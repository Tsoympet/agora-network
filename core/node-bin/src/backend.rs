//! Live [`RpcBackend`] backed by chain admission + mempool.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use agora_p2p::{Mempool, NetworkHandle, NetworkMessage, DEFAULT_TEMPLATE_TX_LIMIT};
use agora_rpc::{RpcBackend, RpcError};
use agora_state_machine::{outpoint_key, validate_mempool_tx, ColumnFamily, StateStore};
use agora_types::{Address, Amount, Block, Hash, OutPoint, Transaction, TxOut};
use borsh::BorshDeserialize;

use crate::admit::ChainState;

/// UTXO + signature + mempool reservation checks, then admit under one lock.
pub(crate) fn admit_transaction(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: Transaction,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    validate_mempool_tx(store, &tx, pool.reserved())
        .map_err(|e| RpcError::Rejected(format!("utxo: {e}")))?;
    pool.admit(tx)
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
}

impl NodeBackend {
    pub fn new(
        chain: Arc<Mutex<ChainState>>,
        store: Arc<StateStore>,
        net: Option<NetworkHandle>,
        allow_fund: bool,
        mempool: Arc<Mutex<Mempool>>,
        miner_address: Address,
    ) -> Self {
        Self {
            chain,
            store,
            mempool,
            net,
            allow_fund,
            fund_nonce: 0,
            miner_address,
        }
    }

    fn utxo_balance(&self, address: &Address) -> Result<Amount, RpcError> {
        let mut total = Amount::ZERO;
        self.store
            .for_each_cf(ColumnFamily::Utxo, |_key, value| {
                let out = TxOut::try_from_slice(value)
                    .map_err(|e| agora_state_machine::StateError::Storage(e.to_string()))?;
                if &out.address == address {
                    total = total.checked_add(out.value).ok_or_else(|| {
                        agora_state_machine::StateError::Storage("balance overflow".into())
                    })?;
                }
                Ok(())
            })
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(total)
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

    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError> {
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
        let key = outpoint_key(&OutPoint {
            tx_id,
            index: 0,
        });
        let bytes =
            borsh::to_vec(&out).map_err(|e| RpcError::Internal(e.to_string()))?;
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
                    RpcError::Rejected(format!("missing parent {h}"))
                }
                crate::admit::AdmitError::Utxo(msg) => {
                    RpcError::Rejected(format!("utxo: {msg}"))
                }
                crate::admit::AdmitError::WrongDifficulty { expected, got } => {
                    RpcError::Rejected(format!(
                        "wrong difficulty: expected bits={expected}, got={got}"
                    ))
                }
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
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0).unwrap(),
        ));
        let miner = Address([1u8; 20]);
        let mut backend = NodeBackend::new(chain.clone(), store, None, false, mempool, miner);
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
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0).unwrap(),
        ));
        let mut backend =
            NodeBackend::new(chain, store, None, false, mempool, Address::ZERO);

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
                    value: Amount::from_whole(1).unwrap(),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(
                        premine.as_base_units() - Amount::from_whole(1).unwrap().as_base_units(),
                    ),
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
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0).unwrap(),
        ));
        let miner = Address([2u8; 20]);
        let mut backend = NodeBackend::new(chain, store, None, false, mempool.clone(), miner);

        let premine = Amount::from_whole(10_000_000).unwrap();
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
                    value: Amount::from_whole(1).unwrap(),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(
                        premine.as_base_units() - Amount::from_whole(1).unwrap().as_base_units(),
                    ),
                    address: from.address(),
                },
            ],
            7,
        );
        sign_transaction(&mut transfer, &from).unwrap();
        let tx_id = backend.submit_transaction(transfer.clone()).unwrap();

        let mut block = backend.get_block_template().unwrap();
        assert_eq!(block.transactions.len(), 2);
        assert!(block.transactions[0].inputs.is_empty());
        assert_eq!(block.transactions[1].tx_id(), tx_id);
        assert_eq!(
            block.header.tx_root,
            Block::compute_tx_root(&block.transactions)
        );
        block.header.nonce = 1;
        let pow = RandomXPowHasher.pow_hash(&block.header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &pow)
            .unwrap();
        backend.submit_block(block).unwrap();
        assert!(!mempool.lock().unwrap().contains(&tx_id));
        assert_eq!(
            backend.get_balance(&to).as_base_units(),
            Amount::from_whole(1).unwrap().as_base_units()
        );
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
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0).unwrap(),
        ));
        let mut backend =
            NodeBackend::new(chain, store.clone(), None, true, mempool, Address::ZERO);

        let drip = Amount::from_base_units(5_000);
        assert_eq!(
            backend.fund_address(funded.address(), drip).unwrap(),
            drip
        );
        assert_eq!(backend.get_balance(&funded.address()), drip);

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

        let mut spend = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: op,
            }],
            vec![TxOut {
                value: drip,
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
        assert_eq!(backend.get_balance(&payee), drip);
    }
}
