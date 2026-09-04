//! Live [`RpcBackend`] backed by chain admission + mempool.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::PowAlgorithm;
use agora_governance::{
    civic_overview_json, list_proposals_json, list_topics_json, office_json, proposal_json,
    ProposalKind, TopicCategory, VoteChoice,
};
use agora_p2p::{
    Mempool, NetworkHandle, NetworkMessage, DEFAULT_MIN_RELAY_FEE, DEFAULT_TEMPLATE_TX_LIMIT,
};
use agora_rpc::{FeeEstimate, MempoolEntry, NodeInfo, RpcBackend, RpcError, TxLookup, UtxoEntry};
use agora_state_machine::{
    apply_account_transfer, apply_drc_payment, apply_ovl_execution, apply_signed_stake_tx,
    build_snapshot, load_epoch, load_reward_pool, load_validator, lookup_tx_location, meta_keys,
    outpoint_key, validate_mempool_tx_with_auth, AccountJournal, ColumnFamily,
    governance_treasury_root, load_canonical_governance_policy, load_protocol_treasuries,
    StakingParams, StateStore, TxAuthContext, WriteBatch,
};
use agora_types::{
    AccountTransfer, Address, Amount, Block, CheckpointAttestation, DrcPaymentTx, Hash,
    NativeAssetId, OutPoint, OvlExecutionTx, SignedStakeTx, Transaction, TxOut,
};
use borsh::BorshDeserialize;
use serde_json::{json, Value};

use crate::admit::ChainState;
use crate::civic::{load_civic, save_civic};

fn min_relay_fee() -> u64 {
    std::env::var("AGORA_MIN_RELAY_FEE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MIN_RELAY_FEE)
}

fn parse_stake_asset(asset: &str) -> Result<NativeAssetId, RpcError> {
    match asset.trim().to_ascii_uppercase().as_str() {
        "OVL" | "OVOLOS" => Ok(NativeAssetId::OVL),
        "DRC" | "DRACHMA" => Ok(NativeAssetId::DRC),
        other => Err(RpcError::InvalidParams(format!(
            "staking asset must be OVL or DRC, got {other}"
        ))),
    }
}

/// UTXO + network-bound signature + mempool reservation checks, then admit.
pub(crate) fn admit_transaction(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: Transaction,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    let fee = validate_mempool_tx_with_auth(store, &tx, pool.reserved(), Some(auth))
        .map_err(|e| RpcError::Rejected(format!("utxo: {e}")))?;
    let min_fee = min_relay_fee();
    if fee < min_fee {
        return Err(RpcError::Rejected(format!(
            "fee too low: {fee} < min relay {min_fee}"
        )));
    }
    // Auth already verified; mempool only tracks reservations / fee market.
    pool.admit_priced(tx, fee)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve an OVL/DRC account transfer under the mempool lock.
pub(crate) fn admit_account_transfer(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: AccountTransfer,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(tx.asset, &tx.from) {
        return Err(RpcError::Rejected(
            "account already has a pending nonce".into(),
        ));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_account_transfer(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|e| RpcError::Rejected(format!("account: {e}")))?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    pool.admit_account(tx)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve a signed stake operation under the shared account nonce.
pub(crate) fn admit_stake_tx(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: SignedStakeTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(tx.asset, &tx.actor) {
        return Err(RpcError::Rejected(
            "account already has a pending nonce".into(),
        ));
    }
    let params = match tx.asset {
        NativeAssetId::OVL => StakingParams::ovl_default(),
        NativeAssetId::DRC => StakingParams::drc_default(),
        NativeAssetId::TLT => return Err(RpcError::Rejected("TLT cannot be staked".into())),
    };
    let mut batch = WriteBatch::new();
    apply_signed_stake_tx(store, &mut batch, &tx, auth, &params)
        .map_err(|e| RpcError::Rejected(format!("stake: {e}")))?;
    pool.admit_stake(tx)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve a signed OVL execution envelope.
pub(crate) fn admit_ovl_execution(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: OvlExecutionTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::OVL, &tx.from) {
        return Err(RpcError::Rejected(
            "OVL account already has a pending nonce".into(),
        ));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_ovl_execution(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|e| RpcError::Rejected(format!("OVL execution: {e}")))?;
    pool.admit_execution(tx)
        .map_err(|e| RpcError::Rejected(e.to_string()))
}

/// Validate and reserve a signed native DRC payment.
pub(crate) fn admit_drc_payment(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcPaymentTx,
    auth: &TxAuthContext,
) -> Result<Hash, RpcError> {
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.from) {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_payment(store, &tx, auth, &mut batch, &mut journal)
        .map_err(|e| RpcError::Rejected(format!("DRC payment: {e}")))?;
    pool.admit_payment(tx)
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

    fn tx_auth(&self) -> TxAuthContext {
        let chain_id = match self.network.to_ascii_lowercase().as_str() {
            "mainnet" => "agora-mainnet-1",
            "testnet" => "agora-testnet-1",
            _ => "agora-dev",
        };
        TxAuthContext {
            chain_id: chain_id.into(),
            genesis: self.genesis_hash,
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

    fn issued_supply(&self) -> u64 {
        self.store
            .get_cf(ColumnFamily::Meta, meta_keys::ISSUED_SUPPLY)
            .ok()
            .flatten()
            .and_then(|b| {
                if b.len() == 8 {
                    Some(u64::from_le_bytes(b.try_into().ok()?))
                } else {
                    None
                }
            })
            .unwrap_or(10_000)
            .max(1)
    }

    fn with_civic<R>(
        &self,
        f: impl FnOnce(&agora_governance::CivicSnapshot) -> Result<R, RpcError>,
    ) -> Result<R, RpcError> {
        let eligible = self.issued_supply();
        let mut snap = load_civic(self.store.as_ref(), eligible)?;
        snap.governance.ecclesia_eligible_power = eligible;
        f(&snap)
    }

    fn with_civic_mut<R>(
        &self,
        f: impl FnOnce(&mut agora_governance::CivicSnapshot) -> Result<R, RpcError>,
    ) -> Result<R, RpcError> {
        let eligible = self.issued_supply();
        let mut snap = load_civic(self.store.as_ref(), eligible)?;
        snap.governance.ecclesia_eligible_power = eligible;
        let out = f(&mut snap)?;
        save_civic(self.store.as_ref(), &snap)?;
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
        let acceptance =
            agora_state_machine::tx_acceptance_status(self.store.as_ref(), &block_id, index)
                .ok()
                .flatten();
        match self
            .chain
            .lock()
            .ok()
            .and_then(|g| g.confirmations(&block_id))
        {
            Some(confirmations) => {
                // Explicit acceptance wins over block color. Missing record =
                // legacy pre-acceptance blocks (treat as confirmed when blue).
                if let Some(status) = acceptance {
                    if !status.is_accepted() {
                        return Ok(TxLookup::orphaned(tx.clone(), block_id, index)
                            .with_acceptance(status.as_str()));
                    }
                    return Ok(
                        TxLookup::confirmed(tx.clone(), block_id, index, confirmations)
                            .with_acceptance(status.as_str()),
                    );
                }
                Ok(TxLookup::confirmed(
                    tx.clone(),
                    block_id,
                    index,
                    confirmations,
                ))
            }
            None => {
                let mut lookup = TxLookup::orphaned(tx.clone(), block_id, index);
                if let Some(status) = acceptance {
                    lookup.acceptance = Some(status.as_str().into());
                }
                Ok(lookup)
            }
        }
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
            chain_id: Some(self.tx_auth().chain_id),
            min_relay_fee: min_relay_fee(),
        })
    }

    fn estimate_fee(&self) -> Result<FeeEstimate, RpcError> {
        let min = min_relay_fee();
        let pool = self
            .mempool
            .lock()
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        // Bitcoin-class guidance: max(min_relay, mempool median) + mild congestion premium.
        let median = pool.median_fee().unwrap_or(min);
        let congestion = (pool.len() as u64).saturating_mul(100);
        let suggested = median.max(min).saturating_add(congestion);
        Ok(FeeEstimate {
            min_relay_fee: min,
            suggested_fee: suggested,
        })
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_transaction(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::Transaction(tx)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn submit_account_transfer(&mut self, tx: AccountTransfer) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_account_transfer(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::AccountTransfer(tx))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(id)
    }

    fn submit_ovl_execution(&mut self, tx: OvlExecutionTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_ovl_execution(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::OvlExecution(tx))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(id)
    }

    fn submit_drc_payment(&mut self, tx: DrcPaymentTx) -> Result<Hash, RpcError> {
        let auth = self.tx_auth();
        let id = admit_drc_payment(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::DrcPayment(tx))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
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
        let (transfers, account_transfers, stake_ops, ovl_executions, drc_payments) = {
            let pool = self
                .mempool
                .lock()
                .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
            (
                pool.select_transfers(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_account_transfers(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_stake_ops(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_ovl_executions(DEFAULT_TEMPLATE_TX_LIMIT),
                pool.select_drc_payments(DEFAULT_TEMPLATE_TX_LIMIT),
            )
        };
        self.chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .block_template_lanes(
                self.miner_address,
                &transfers,
                &account_transfers,
                &stake_ops,
                &ovl_executions,
                &drc_payments,
            )
            .map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn randomx_epoch(&self, parents: &[Hash]) -> u64 {
        self.chain
            .lock()
            .map(|c| c.randomx_epoch_for_parents(parents))
            .unwrap_or(0)
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
                crate::admit::AdmitError::FinalityReorg {
                    finalized,
                    abandoned,
                } => RpcError::Rejected(format!(
                    "reorg beyond finality: abandoned {abandoned} <= finalized {finalized}"
                )),
                crate::admit::AdmitError::InvalidAttestation(msg) => {
                    RpcError::Rejected(format!("attestation: {msg}"))
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

    fn get_finality(&self, block_hash: &Hash) -> Result<Value, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let cert = chain
            .finality_certificate(block_hash)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let finalized_tip = chain
            .finalized_blue_score()
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        match cert {
            Some(c) => Ok(json!({
                "block_hash": c.body.block_hash.to_hex(),
                "blue_score": c.body.blue_score,
                "state": c.state.as_str(),
                "pow_work_met": c.pow_work_met,
                "ovl_signed_stake": c.ovl_signed_stake,
                "ovl_active_stake": c.ovl_active_stake,
                "drc_signed_stake": c.drc_signed_stake,
                "drc_active_stake": c.drc_active_stake,
                "finalized": c.state.is_finalized(),
                "finalized_tip_blue_score": finalized_tip,
            })),
            None => Ok(json!({
                "block_hash": block_hash.to_hex(),
                "state": "Proposed",
                "pow_work_met": false,
                "finalized": false,
                "finalized_tip_blue_score": finalized_tip,
            })),
        }
    }

    fn get_finalized_tip(&self) -> Result<Value, RpcError> {
        let chain = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?;
        let score = chain
            .finalized_blue_score()
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({ "blue_score": score }))
    }

    fn submit_attestation(&mut self, attestation: Value) -> Result<Value, RpcError> {
        let att: CheckpointAttestation = serde_json::from_value(attestation)
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        let cert = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .admit_attestation(att.clone())
            .map_err(|e| match e {
                crate::admit::AdmitError::InvalidAttestation(msg) => {
                    RpcError::Rejected(format!("attestation: {msg}"))
                }
                other => RpcError::Rejected(other.to_string()),
            })?;
        if let Some(net) = &self.net {
            let _ = net.publish_message(NetworkMessage::CheckpointAttestation(att));
        }
        Ok(json!({
            "block_hash": cert.body.block_hash.to_hex(),
            "state": cert.state.as_str(),
            "finalized": cert.state.is_finalized(),
            "ovl_signed_stake": cert.ovl_signed_stake,
            "drc_signed_stake": cert.drc_signed_stake,
        }))
    }

    fn get_validator_set(&self, asset: &str, epoch: Option<u64>) -> Result<Value, RpcError> {
        let asset = parse_stake_asset(asset)?;
        let epoch = match epoch {
            Some(e) => e,
            None => load_epoch(self.store.as_ref(), asset)
                .map_err(|e| RpcError::Internal(e.to_string()))?,
        };
        let snap = build_snapshot(self.store.as_ref(), asset, epoch)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "asset": asset.ticker(),
            "epoch": snap.epoch,
            "total_active_stake": snap.total_active_stake,
            "commitment": snap.commitment().to_hex(),
            "validators": snap.validators.iter().map(|(a, p)| json!({
                "operator": a.to_bech32(),
                "voting_power": p,
            })).collect::<Vec<_>>(),
        }))
    }

    fn get_validator(&self, asset: &str, operator: &Address) -> Result<Value, RpcError> {
        let asset = parse_stake_asset(asset)?;
        let Some(val) = load_validator(self.store.as_ref(), asset, operator)
            .map_err(|e| RpcError::Internal(e.to_string()))?
        else {
            return Err(RpcError::NotFound(format!(
                "validator {}/{}",
                asset.ticker(),
                operator.to_bech32()
            )));
        };
        Ok(json!({
            "asset": asset.ticker(),
            "operator": val.operator.to_bech32(),
            "withdrawal": val.withdrawal.to_bech32(),
            "self_bond": val.self_bond,
            "delegated": val.delegated,
            "commission_bps": val.commission_bps,
            "status": format!("{:?}", val.status),
            "jailed_until_epoch": val.jailed_until_epoch,
        }))
    }

    fn get_reward_pool(&self, asset: &str) -> Result<Value, RpcError> {
        let asset = parse_stake_asset(asset)?;
        let amount = load_reward_pool(self.store.as_ref(), asset)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "asset": asset.ticker(),
            "amount": amount,
        }))
    }

    fn get_protocol_treasuries(&self) -> Result<Value, RpcError> {
        let policy = load_canonical_governance_policy(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let root = governance_treasury_root(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        let treasuries = load_protocol_treasuries(self.store.as_ref())
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(json!({
            "maturity": "Scaffold",
            "consensus_mutations_active": false,
            "governance_root": root.to_hex(),
            "policy": {
                "version": policy.version,
                "constitution_id": policy.constitution_id,
                "constitution_hash": policy.constitution_hash.to_hex(),
                "authorization_root": policy.authorization_root.to_hex(),
            },
            "treasuries": treasuries.iter().map(|t| json!({
                "id": t.treasury.as_str(),
                "asset": t.asset.ticker(),
                "balance": t.balance.as_base_units(),
            })).collect::<Vec<_>>(),
        }))
    }

    fn submit_stake_tx(&mut self, stake_tx: Value) -> Result<Value, RpcError> {
        let tx: SignedStakeTx =
            serde_json::from_value(stake_tx).map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        let auth = self.tx_auth();
        let id = admit_stake_tx(&self.store, &self.mempool, tx.clone(), &auth)?;
        if let Some(net) = &self.net {
            net.publish_message(NetworkMessage::StakeTx(tx.clone()))
                .map_err(|e| RpcError::Internal(e.to_string()))?;
        }
        Ok(json!({
            "stake_tx_id": id.to_hex(),
            "kind": tx.kind.as_str(),
            "asset": tx.asset.ticker(),
            "actor": tx.actor.to_bech32(),
            "validator": tx.validator.to_bech32(),
            "amount": tx.amount,
            "nonce": tx.nonce,
            "path": "mempool",
        }))
    }

    fn get_constitution(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            Ok(json!({
                "id": snap.governance.constitution.id,
                "content_hash": snap.governance.constitution.content_hash_hex(),
                "body_markdown": snap.governance.constitution.body_markdown,
            }))
        })
    }

    fn get_governance(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            let mut value = civic_overview_json(snap);
            if let Some(object) = value.as_object_mut() {
                object.insert("scope".into(), json!("administrative_local"));
                object.insert("consensus_accepted".into(), json!(false));
            }
            Ok(value)
        })
    }

    fn list_proposals(&self, limit: usize) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(list_proposals_json(&snap.governance, limit)))
    }

    fn get_proposal(&self, id: u64) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            let p = snap
                .governance
                .proposal(id)
                .ok_or_else(|| RpcError::NotFound(format!("proposal {id}")))?;
            Ok(proposal_json(p))
        })
    }

    fn list_offices(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            Ok(json!({
                "offices": snap.governance.offices.seats.iter().map(office_json).collect::<Vec<_>>(),
            }))
        })
    }

    fn list_forum_topics(&self, limit: usize) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(list_topics_json(&snap.community, limit)))
    }

    fn submit_proposal(
        &mut self,
        author: Address,
        title: String,
        summary: String,
        kind: ProposalKind,
        slot: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap
                .governance
                .submit_proposal(author, title, summary, kind, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id }))
        })
    }

    fn deposit_proposal(&mut self, id: u64, amount: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .add_deposit(id, amount)
                .map_err(crate::civic::map_gov_err)?;
            let p = snap
                .governance
                .proposal(id)
                .ok_or_else(|| RpcError::NotFound(format!("proposal {id}")))?;
            Ok(json!({ "proposal_id": id, "deposit": p.deposit }))
        })
    }

    fn open_proposal_voting(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .open_voting(id, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": "voting" }))
        })
    }

    fn cast_gov_vote(
        &mut self,
        id: u64,
        voter: Address,
        choice: VoteChoice,
        raw_balance: u64,
        total_supply: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .cast_vote(id, voter, choice, raw_balance, total_supply)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "voted": true }))
        })
    }

    fn tally_proposal(&mut self, id: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let status = snap
                .governance
                .tally(id)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": status }))
        })
    }

    fn enter_proposal_timelock(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .enter_timelock(id, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": "timelock" }))
        })
    }

    fn execute_proposal(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .execute(id, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "status": "executed" }))
        })
    }

    fn post_forum_topic(
        &mut self,
        author: Address,
        title: String,
        body: String,
        category: TopicCategory,
        slot: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap
                .community
                .post_topic(author, title, body, category, slot)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "topic_id": id }))
        })
    }

    fn ack_constitution(&mut self, address: Address, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap.governance.constitution.id.clone();
            let hash = snap.governance.constitution.content_hash_hex();
            snap.community
                .acknowledge_constitution(address, id.clone(), hash.clone(), slot);
            Ok(json!({
                "address": address.to_bech32(),
                "constitution_id": id,
                "constitution_hash": hash,
                "acked": true,
            }))
        })
    }

    fn sponsor_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .sponsor_as_tamias(id, who)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "sponsored": true }))
        })
    }

    fn assent_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .record_archon_assent(id, who)
                .map_err(crate::civic::map_gov_err)?;
            Ok(json!({ "proposal_id": id, "assented": true }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::ChainState;
    use agora_consensus::{PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
    use agora_crypto::{
        derive_bip44, seed_from_mnemonic, sign_account_transfer_bound, sign_drc_payment_bound,
        sign_ovl_execution_bound, sign_transaction_bound, Bip44Path,
    };
    use agora_state_machine::{credit_account_into, ColumnFamily, GenesisBuilder};
    use agora_types::{Address, Block, OutPoint, TxIn, TxOut};
    use borsh::BorshDeserialize;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn account_transfer_enters_template_lane() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(100),
        )
        .unwrap();
        store.write_batch(funding).unwrap();

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
        let mut tx = AccountTransfer::unsigned_with_fee(
            NativeAssetId::OVL,
            alice.address(),
            bob.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            0,
        );
        sign_account_transfer_bound(&mut tx, &alice, "agora-dev", &genesis).unwrap();

        let id = backend.submit_account_transfer(tx.clone()).unwrap();
        assert_eq!(id, tx.transfer_id());
        let template = backend.get_block_template().unwrap();
        assert_eq!(template.account_transfers, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
        assert_ne!(
            template.header.tx_root,
            Block::compute_tx_root(&template.transactions)
        );
    }

    #[test]
    fn ovl_execution_enters_template_lane() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(50_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
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
        let mut tx = OvlExecutionTx::unsigned(
            alice.address(),
            bob.address(),
            Amount::from_base_units(1_000),
            agora_state_machine::OVL_INTRINSIC_GAS,
            1,
            0,
            vec![],
        );
        sign_ovl_execution_bound(&mut tx, &alice, "agora-dev", &genesis).unwrap();

        let id = backend.submit_ovl_execution(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        let template = backend.get_block_template().unwrap();
        assert_eq!(template.ovl_executions, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
    }

    #[test]
    fn drc_payment_enters_template_lane() {
        let store = Arc::new(StateStore::open_in_memory());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &alice.address(),
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
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
        let mut tx = DrcPaymentTx::unsigned(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(100),
            Amount::from_base_units(1),
            77,
            Hash([8; 32]),
            0,
        );
        sign_drc_payment_bound(&mut tx, &alice, "agora-dev", &genesis).unwrap();

        let id = backend.submit_drc_payment(tx.clone()).unwrap();
        assert_eq!(id, tx.payment_id());
        let template = backend.get_block_template().unwrap();
        assert_eq!(template.drc_payments, vec![tx]);
        assert_eq!(template.header.tx_root, template.compute_body_root());
    }

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
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-dev".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
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
        sign_transaction_bound(&mut bad, &from, "agora-dev", &genesis).unwrap();
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
        sign_transaction_bound(&mut good, &from, "agora-dev", &genesis).unwrap();
        let id = backend.submit_transaction(good.clone()).unwrap();
        assert_eq!(id, good.tx_id());
        // Second spend of the same outpoint must fail while the first is reserved.
        let mut conflict = good.clone();
        conflict.nonce = 3;
        sign_transaction_bound(&mut conflict, &from, "agora-dev", &genesis).unwrap();
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
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-dev".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
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
        sign_transaction_bound(&mut transfer, &from, "agora-dev", &genesis).unwrap();
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
        assert_eq!(confirmed.acceptance.as_deref(), Some("Accepted"));
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
            ChainState::bootstrap_with(
                store.clone(),
                genesis,
                crate::admit::ChainBootConfig {
                    chain_id: "agora-dev".into(),
                    ..crate::admit::ChainBootConfig::default()
                },
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
        sign_transaction_bound(&mut spend, &funded, "agora-dev", &genesis).unwrap();
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
