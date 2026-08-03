//! Live [`RpcBackend`] backed by chain admission + mempool.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::PowAlgorithm;
use agora_p2p::{
    Mempool, NetworkHandle, NetworkMessage, DEFAULT_MIN_RELAY_FEE, DEFAULT_TEMPLATE_TX_LIMIT,
};
use agora_rpc::{MempoolEntry, NodeInfo, RpcBackend, RpcError, TxLookup, UtxoEntry};
use agora_state_machine::{
    lookup_tx_location, outpoint_key, validate_mempool_tx, ColumnFamily, StateStore,
};
use agora_types::{Address, Amount, Block, Hash, OutPoint, Transaction, TxOut};
use borsh::BorshDeserialize;

use crate::admit::ChainState;

fn min_relay_fee() -> u64 {
    std::env::var("AGORA_MIN_RELAY_FEE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MIN_RELAY_FEE)
}

/// UTXO + signature + mempool reservation checks, then admit under one lock.
pub(crate) fn admit_transaction(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: Transaction,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let fee = validate_mempool_tx(store, &tx, pool.reserved())
        .map_err(|e| RpcError::Rejected(format!("utxo: {e}")))?;
    let min_fee = min_relay_fee();
    if fee < min_fee {
        return Err(RpcError::Rejected(format!(
            "fee too low: {fee} < min relay {min_fee}"
        )));
    }
    pool.admit_priced(tx, fee)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Node RPC surface: tips/blocks from store, signed tx → mempool + gossip.
pub struct NodeBackend {
    chain: Arc<Mutex<ChainState>>,
    store: Arc<StateStore>,
    mempool: Arc<Mutex<Mempool>>,
    net: Option<NetworkHandle>,
    /// When true, `agora_fundAddress` mints spendable `cf_utxo` credits (testnet).
    allow_fund: bool,
    /// Monotonic nonce so faucet mints never collide on outpoint keys.
    fund_nonce: u64,
    /// Coinbase payout address for `agora_getBlockTemplate`.
    miner_address: Address,
    /// Live connected-peer count (updated from the p2p event loop).
    connected_peers: Arc<AtomicU32>,
    /// `AGORA_NETWORK` label (`dev` / `testnet` / …).
    network: String,
    /// Block 0 id for this datadir.
    genesis_hash: Hash,
}

impl NodeBackend {
    pub fn new(
        chain: Arc<Mutex<ChainState>>,
        store: Arc<StateStore>,
        net: Option<NetworkHandle>,
        allow_fund: bool,
        mempool: Arc<Mutex<Mempool>>,
        miner_address: Address,
        connected_peers: Arc<AtomicU32>,
        network: impl Into<String>,
        genesis_hash: Hash,
    ) -> Self {
        Self {
            chain,
            store,
            mempool,
            net,
            allow_fund,
            fund_nonce: 0,
            miner_address,
            connected_peers,
            network: network.into(),
            genesis_hash,
        }
    }

    fn utxo_balance(&self, address: &Address) -> Result<Amount, RpcError> {
        let mut total = Amount::ZERO;
        for entry in self.list_utxos(address)? {
            total = total
                .checked_add(entry.value)
                .ok_or_else(|| RpcError::Internal("balance overflow".into()))?;
        }
        Ok(total)
    }

    fn list_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError> {
        let mut out = Vec::new();
        self.store
            .for_each_cf(ColumnFamily::Utxo, |key, value| {
                if key.len() != 36 {
                    return Ok(());
                }
                let tx_out = TxOut::try_from_slice(value)
                    .map_err(|e| agora_state_machine::StateError::Storage(e.to_string()))?;
                if &tx_out.address != address {
                    return Ok(());
                }
                let mut tx_bytes = [0u8; 32];
                tx_bytes.copy_from_slice(&key[..32]);
                let index = u32::from_le_bytes(key[32..36].try_into().unwrap());
                out.push(UtxoEntry {
                    outpoint: OutPoint {
                        tx_id: Hash(tx_bytes),
                        index,
                    },
                    value: tx_out.value,
                });
                Ok(())
            })
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(out)
    }
}

impl RpcBackend for NodeBackend {
    fn dag_tips(&self) -> Vec<Hash> {
        self.chain
            .lock()
            .ok()
            .and_then(|g| g.tips().ok())
            .unwrap_or_default()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        self.chain
            .lock()
            .ok()
            .and_then(|g| g.load_block(hash).ok())
            .flatten()
    }

    fn get_transaction(&self, tx_id: &Hash) -> Result<TxLookup, RpcError> {
        {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            if let Some(tx) = pool.get(tx_id) {
                return Ok(TxLookup::pending(tx.clone(), pool.fee_of(tx_id)));
            }
        }
        let Some((block_id, index)) = lookup_tx_location(self.store.as_ref(), tx_id)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        else {
            return Ok(TxLookup::unknown(*tx_id));
        };
        let block = self.get_block(&block_id);
        let Some(block) = block else {
            return Ok(TxLookup::unknown(*tx_id));
        };
        let Some(tx) = block.transactions.get(index as usize) else {
            return Ok(TxLookup::unknown(*tx_id));
        };
        let confirmations = self
            .chain
            .lock()
            .ok()
            .and_then(|g| g.confirmations(&block_id))
            .unwrap_or(1);
        Ok(TxLookup::confirmed(
            tx.clone(),
            block_id,
            index,
            confirmations,
        ))
    }

    fn get_mempool(&self, limit: usize) -> Result<Vec<MempoolEntry>, RpcError> {
        let pool = self
            .mempool
            .lock()
            .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
        Ok(pool
            .pending_entries(limit)
            .into_iter()
            .map(|(tx, fee)| MempoolEntry {
                tx_id: tx.tx_id(),
                fee: Some(fee),
                transaction: tx,
            })
            .collect())
    }

    fn get_node_info(&self) -> Result<NodeInfo, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let tips = chain.tips().unwrap_or_default();
        let storage = chain.storage_policy();
        let bits = chain.difficulty().as_bits();
        let pow = match chain.pow_algorithm() {
            PowAlgorithm::RandomX => "randomx",
            PowAlgorithm::KHeavyHash => "kheavyhash",
        };
        let mempool_count = self.mempool.lock().map(|p| p.len()).unwrap_or(0);
        Ok(NodeInfo {
            network: self.network.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            peer_id: self.net.as_ref().map(|n| n.peer_id().to_string()),
            connected_peers: Some(self.connected_peers.load(Ordering::Relaxed)),
            tip_count: tips.len(),
            mempool_count,
            pow_algorithm: pow.into(),
            bits,
            archival: storage.archival,
            hot_window: storage.hot_window,
            allow_fund: self.allow_fund,
            miner_address: Some(self.miner_address.to_bech32()),
            genesis_hash: Some(self.genesis_hash.to_hex()),
        })
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError> {
        let id = admit_transaction(&self.store, &self.mempool, tx.clone())?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::Transaction(tx)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_balance(&self, address: &Address) -> Amount {
        self.utxo_balance(address).unwrap_or(Amount::ZERO)
    }

    fn get_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError> {
        self.list_utxos(address)
    }

    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError> {
        if self.network.eq_ignore_ascii_case("mainnet") {
            return Err(RpcError::Rejected(
                "agora_fundAddress is permanently disabled on mainnet".into(),
            ));
        }
        if !self.allow_fund {
            return Err(RpcError::Rejected(
                "agora_fundAddress disabled (set AGORA_RPC_ALLOW_FUND=1 for testnet)".into(),
            ));
        }
        if amount.as_base_units() == 0 {
            return Err(RpcError::InvalidParams("amount must be > 0".into()));
        }
        self.fund_nonce = self.fund_nonce.saturating_add(1);
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // Synthetic outpoint — testnet mint only; not a consensus coinbase.
        let tx_id = Hash::hash_borsh(&(
            b"agora_fund",
            address,
            amount.as_base_units(),
            self.fund_nonce,
            timestamp_ms,
        ));
        let out = TxOut {
            value: amount,
            address,
        };
        let key = outpoint_key(&OutPoint { tx_id, index: 0 });
        let bytes = borsh::to_vec(&out).map_err(|e| RpcError::Internal(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Utxo, &key, &bytes)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        self.utxo_balance(&address)
    }

    fn get_block_template(&self) -> Result<Block, RpcError> {
        let transfers = self
            .mempool
            .lock()
            .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?
            .select_transfers(DEFAULT_TEMPLATE_TX_LIMIT);
        self.chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .block_template(self.miner_address, &transfers)
            .map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError> {
        let id = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .admit_block(block.clone())
            .map_err(|e| match e {
                crate::admit::AdmitError::InvalidPow => {
                    RpcError::Rejected("invalid proof of work".into())
                }
                crate::admit::AdmitError::Duplicate(h) => {
                    RpcError::Rejected(format!("duplicate block {h}"))
                }
                crate::admit::AdmitError::MissingParent(h) => {
                    RpcError::Rejected(format!("missing parent {}", h.to_hex()))
                }
                crate::admit::AdmitError::Utxo(msg) => RpcError::Rejected(format!("utxo: {msg}")),
                crate::admit::AdmitError::WrongDifficulty { expected, got } => RpcError::Rejected(
                    format!("wrong difficulty: expected bits={expected}, got={got}"),
                ),
                crate::admit::AdmitError::BadTxRoot => {
                    RpcError::Rejected("tx_root mismatch".into())
                }
                other => RpcError::Internal(other.to_string()),
            })?;
        if let Ok(mut pool) = self.mempool.lock() {
            pool.evict_for_block(&block);
        }
        if let Some(net) = &self.net {
            // Prefer compact + announce; peers inflate from mempool or issue GetBlock.
            let _ = net.publish_message(NetworkMessage::compact_from_block(&block));
            let _ = net.publish_message(NetworkMessage::BlockAnnounce { hash: id });
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::ChainState;
    use agora_consensus::{PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_state_machine::{ColumnFamily, GenesisBuilder};
    use agora_types::{Address, Block, OutPoint, TxIn, TxOut};
    use borsh::BorshDeserialize;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn genesis_tips_and_admit_easy_block() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let premine = Address([9u8; 20]);
        let genesis = GenesisBuilder::default()
            .with_premine_address(premine)
            .ignite(&store)
            .unwrap();

        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let miner = Address([1u8; 20]);
        let mut backend = NodeBackend::new(
            chain.clone(),
            store,
            None,
            false,
            mempool,
            miner,
            Arc::new(AtomicU32::new(0)),
            "dev",
            genesis,
        );
        assert_eq!(backend.dag_tips(), vec![genesis]);
        assert_eq!(
            backend.get_balance(&premine).as_base_units(),
            Amount::from_whole(10_000_000).unwrap().as_base_units()
        );

        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.header.bits, 0); // DAA initial bits from bootstrap
        assert_eq!(block.transactions.len(), 1);
        assert!(block.transactions[0].inputs.is_empty());
        assert_eq!(block.transactions[0].outputs[0].address, miner);
        assert_eq!(
            block.header.tx_root,
            Block::compute_tx_root(&block.transactions)
        );
        block.header.nonce = 1;
        let pow = RandomXPowHasher.pow_hash(&block.header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &pow)
            .unwrap();
        let reward = block.transactions[0].outputs[0].value;
        let id = backend.submit_block(block).unwrap();
        assert_ne!(id, genesis);
        assert!(backend.dag_tips().contains(&id));
        assert_eq!(backend.get_balance(&miner), reward);
    }

    #[test]
    fn submit_transaction_requires_live_utxo() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let genesis = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis_block = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis_block.transactions[0].tx_id();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(
            chain,
            store,
            None,
            false,
            mempool,
            Address::ZERO,
            Arc::new(AtomicU32::new(0)),
            "dev",
            genesis,
        );

        let mut bad = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: to,
            }],
            1,
        );
        sign_transaction(&mut bad, &from).unwrap();
        assert!(backend.submit_transaction(bad).is_err());

        let premine = Amount::from_whole(10_000_000).unwrap();
        let pay = Amount::from_whole(1).unwrap().as_base_units();
        let fee = 1u64;
        let mut good = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![
                TxOut {
                    value: Amount::from_base_units(pay),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(premine.as_base_units() - pay - fee),
                    address: from.address(),
                },
            ],
            2,
        );
        sign_transaction(&mut good, &from).unwrap();
        let id = backend.submit_transaction(good.clone()).unwrap();
        assert_eq!(id, good.tx_id());
        // Second spend of the same outpoint must fail while the first is reserved.
        let mut conflict = good.clone();
        conflict.nonce = 3;
        sign_transaction(&mut conflict, &from).unwrap();
        assert!(backend.submit_transaction(conflict).is_err());
    }

    #[test]
    fn template_includes_mempool_tx_and_evicts_on_submit() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let genesis = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis_block = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis_block.transactions[0].tx_id();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let miner = Address([2u8; 20]);
        let mut backend = NodeBackend::new(
            chain,
            store,
            None,
            false,
            mempool.clone(),
            miner,
            Arc::new(AtomicU32::new(0)),
            "dev",
            genesis,
        );

        let premine = Amount::from_whole(10_000_000).unwrap();
        let pay = Amount::from_whole(1).unwrap().as_base_units();
        let fee = 1u64;
        let mut transfer = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![
                TxOut {
                    value: Amount::from_base_units(pay),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(premine.as_base_units() - pay - fee),
                    address: from.address(),
                },
            ],
            7,
        );
        sign_transaction(&mut transfer, &from).unwrap();
        let tx_id = backend.submit_transaction(transfer.clone()).unwrap();

        let pending = backend.get_transaction(&tx_id).unwrap();
        assert_eq!(pending.status.as_str(), "pending");
        assert_eq!(pending.fee, Some(fee));

        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.transactions.len(), 2);
        assert!(block.transactions[0].inputs.is_empty());
        assert_eq!(block.transactions[1].tx_id(), tx_id);
        let coinbase_value = block.transactions[0].outputs[0].value.as_base_units();
        // Next block after genesis (blue_score 1) estimates blue_score 2.
        let emission = agora_consensus::EmissionSchedule::default().reward_at_blue_score(2);
        assert_eq!(
            coinbase_value,
            emission + fee,
            "coinbase should be emission + transfer fee"
        );
        assert_eq!(
            block.header.tx_root,
            Block::compute_tx_root(&block.transactions)
        );
        block.header.nonce = 1;
        let pow = RandomXPowHasher.pow_hash(&block.header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &pow)
            .unwrap();
        let block_id = backend.submit_block(block).unwrap();
        assert!(!mempool.lock().unwrap().contains(&tx_id));
        assert_eq!(
            backend.get_balance(&to).as_base_units(),
            Amount::from_whole(1).unwrap().as_base_units()
        );
        assert_eq!(backend.get_balance(&miner).as_base_units(), emission + fee);

        let confirmed = backend.get_transaction(&tx_id).unwrap();
        assert_eq!(confirmed.status.as_str(), "confirmed");
        assert_eq!(confirmed.block_id, Some(block_id));
        assert_eq!(confirmed.index, Some(1));
    }

    #[test]
    fn fund_address_mints_spendable_utxo() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let funded = derive_bip44(&seed, &Bip44Path::external(5)).unwrap();
        let payee = derive_bip44(&seed, &Bip44Path::external(6))
            .unwrap()
            .address();
        let genesis = GenesisBuilder::default()
            .with_premine_address(Address([9u8; 20]))
            .ignite(&store)
            .unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        let mut backend = NodeBackend::new(
            chain,
            store.clone(),
            None,
            true,
            mempool,
            Address::ZERO,
            Arc::new(AtomicU32::new(0)),
            "dev",
            genesis,
        );

        let drip = Amount::from_base_units(5_000);
        assert_eq!(backend.fund_address(funded.address(), drip).unwrap(), drip);
        assert_eq!(backend.get_balance(&funded.address()), drip);
        let minted = backend.get_utxos(&funded.address()).unwrap();
        assert_eq!(minted.len(), 1);
        assert_eq!(minted[0].value, drip);

        let (op, out) = {
            let mut found = None;
            store
                .for_each_cf(ColumnFamily::Utxo, |key, value| {
                    let tx_out = TxOut::try_from_slice(value)
                        .map_err(|e| agora_state_machine::StateError::Storage(e.to_string()))?;
                    if tx_out.address == funded.address() && key.len() == 36 {
                        let mut tx_bytes = [0u8; 32];
                        tx_bytes.copy_from_slice(&key[..32]);
                        let index = u32::from_le_bytes(key[32..36].try_into().unwrap());
                        found = Some((
                            OutPoint {
                                tx_id: Hash(tx_bytes),
                                index,
                            },
                            tx_out,
                        ));
                    }
                    Ok(())
                })
                .unwrap();
            found.expect("minted utxo")
        };
        assert_eq!(out.value, drip);

        let fee = 1u64;
        let mut spend = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: op,
            }],
            vec![TxOut {
                value: Amount::from_base_units(drip.as_base_units() - fee),
                address: payee,
            }],
            1,
        );
        sign_transaction(&mut spend, &funded).unwrap();
        backend.submit_transaction(spend).unwrap();
        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.transactions.len(), 2);
        block.header.nonce = 1;
        backend.submit_block(block).unwrap();
        assert_eq!(backend.get_balance(&funded.address()), Amount::ZERO);
        assert_eq!(
            backend.get_balance(&payee).as_base_units(),
            drip.as_base_units() - fee
        );
    }

    #[test]
    fn fund_address_hard_disabled_on_mainnet_label() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(
                store.clone(),
                genesis,
                PowAlgorithm::RandomX,
                0,
                crate::storage_policy::StoragePolicy::default(),
            )
            .unwrap(),
        ));
        // Even with allow_fund=true, mainnet label must reject.
        let mut backend = NodeBackend::new(
            chain,
            store,
            None,
            true,
            mempool,
            Address::ZERO,
            Arc::new(AtomicU32::new(0)),
            "mainnet",
            genesis,
        );
        let err = backend
            .fund_address(Address([1u8; 20]), Amount::from_base_units(1))
            .unwrap_err();
        match err {
            RpcError::Rejected(msg) => assert!(msg.contains("mainnet")),
            other => panic!("unexpected {other:?}"),
        }
    }
}
