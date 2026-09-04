//! Block admission: parent-contextual DAA + PoW → full blue_order UTXO proof →
//! atomic DAG persist → virtual UTXO reorg.
//!
//! Finality hooks live in [`finality`].
//!
//! **Conflict model:** before any durable mutation, the node proves that
//! `blue_order(candidate)` (selected-parent blues + newly accepted merge-set
//! blues + the candidate) is a valid *virtual* UTXO transition. Blue-block
//! acceptance is distinct from transaction acceptance: duplicate or conflicting
//! sibling transfers are skipped after the first spend in consensus order wins.
//! The merge block itself is not rejected merely because two otherwise-valid
//! blues contain the same mempool tx or conflicting spends.
//!
//! Live `cf_utxo` follows blues of `order_past(virtual_tip)` (tip by cumulative
//! blue work). Consensus `header.bits` come from the selected-parent DAA window.

mod finality;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::{
    median_time_past, next_difficulty_weighted, work_from_bits, ConsensusLimits, DaaConfig,
    DaaSample, Dag, Difficulty, EmissionSchedule, Ghostdag, GhostdagConfig, GhostdagSnapshot,
    KHeavyHashPowHasher, LeadingZeroPow, PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher,
};
use agora_state_machine::{
    apply_block_batched_virtual, ghostdag_key, index_block_transactions_into, list_tx_inclusions,
    load_ghostdag_record, load_header, load_utxo_journal, lookup_tx_location, meta_keys,
    revert_journal_batched, set_primary_tx_location, store_ghostdag_record, store_header,
    store_header_into, sum_transfer_fees, utxo_diff_key, ColumnFamily, GhostdagRecord, StateStore,
    TxAuthContext, WriteBatch,
};
use agora_types::{Address, Amount, Block, BlockHeader, Hash, Transaction, TxOut};
use thiserror::Error;
use tracing::{debug, warn};

use crate::storage_policy::StoragePolicy;

#[derive(Debug, Error)]
pub enum AdmitError {
    #[error("invalid proof of work")]
    InvalidPow,
    #[error("wrong difficulty: expected bits={expected}, got={got}")]
    WrongDifficulty { expected: u32, got: u32 },
    #[error("missing parent: {}", .0.to_hex())]
    MissingParent(Hash),
    #[error("duplicate block: {0}")]
    Duplicate(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("consensus: {0}")]
    Consensus(String),
    #[error("utxo: {0}")]
    Utxo(String),
    #[error("tx_root mismatch")]
    BadTxRoot,
    #[error("too many parents: {got} > {max}")]
    TooManyParents { got: usize, max: usize },
    #[error("too few parents")]
    TooFewParents,
    #[error("duplicate parent: {0}")]
    DuplicateParent(String),
    #[error("block too large: {got} > {max}")]
    BlockTooLarge { got: usize, max: usize },
    #[error("too many transactions: {got} > {max}")]
    TooManyTransactions { got: usize, max: usize },
    #[error("tx {tx_index} too large: {got} > {max}")]
    TxTooLarge {
        tx_index: usize,
        got: usize,
        max: usize,
    },
    #[error("tx {tx_index} too many inputs: {got} > {max}")]
    TooManyTxInputs {
        tx_index: usize,
        got: usize,
        max: usize,
    },
    #[error("tx {tx_index} too many outputs: {got} > {max}")]
    TooManyTxOutputs {
        tx_index: usize,
        got: usize,
        max: usize,
    },
    #[error("timestamp too far ahead: {ts} > {max}")]
    TimestampTooFarAhead { ts: u64, max: u64 },
    #[error("timestamp before parent: {ts} < {parent_ts}")]
    TimestampBeforeParent { ts: u64, parent_ts: u64 },
    #[error("immature coinbase: {0}")]
    ImmatureCoinbase(String),
    #[error("supply cap exceeded")]
    SupplyCapExceeded,
    #[error("reorg beyond finality: abandoned blue_score {abandoned} <= finalized {finalized}")]
    FinalityReorg { finalized: u64, abandoned: u64 },
    #[error("invalid attestation: {0}")]
    InvalidAttestation(String),
}

/// Shared chain state mutated by RPC submit and gossip admission.
pub struct ChainState {
    store: Arc<StateStore>,
    genesis: Hash,
    dag: Dag,
    ghostdag: Ghostdag,
    pow: LeadingZeroPow,
    emission: EmissionSchedule,
    daa: DaaConfig,
    difficulty: Difficulty,
    storage: StoragePolicy,
    limits: ConsensusLimits,
    /// When set, transfer signatures must be network-bound to this identity.
    auth: Option<TxAuthContext>,
    /// Canonical consensus-policy hash bound into checkpoint bodies.
    consensus_policy_hash: Hash,
}

/// Runtime consensus knobs loaded from [`agora_state_machine::ChainParams`].
#[derive(Debug, Clone)]
pub struct ChainBootConfig {
    pub pow: PowAlgorithm,
    pub initial_bits: u32,
    pub daa: DaaConfig,
    pub ghostdag: GhostdagConfig,
    pub emission: EmissionSchedule,
    /// `agora-testnet-1` / … — required for bound tx signatures in production.
    pub chain_id: String,
    /// Bound into Trident checkpoint bodies (from [`agora_state_machine::GenesisConsensusPolicy`]).
    pub consensus_policy_hash: Hash,
}

impl Default for ChainBootConfig {
    fn default() -> Self {
        Self {
            pow: PowAlgorithm::RandomX,
            initial_bits: 0,
            daa: DaaConfig {
                min_level: 0,
                ..DaaConfig::default()
            },
            ghostdag: GhostdagConfig::default(),
            emission: EmissionSchedule::default(),
            chain_id: String::new(),
            consensus_policy_hash: Hash::ZERO,
        }
    }
}

impl From<&agora_state_machine::ChainParams> for ChainBootConfig {
    fn from(params: &agora_state_machine::ChainParams) -> Self {
        let policy = agora_state_machine::GenesisArtifact::from_params(params);
        let consensus_policy_hash =
            Hash::from_hex(&policy.consensus_policy_hash).unwrap_or(Hash::ZERO);
        Self {
            pow: params.pow_algorithm,
            initial_bits: params.bits,
            daa: params.daa.clone(),
            ghostdag: params.ghostdag_config(),
            emission: params.emission.clone(),
            chain_id: params.network.chain_id().into(),
            consensus_policy_hash,
        }
    }
}

impl ChainState {
    /// Convenience bootstrap used by integration tests and RPC backends.
    #[allow(dead_code)]
    pub fn bootstrap(
        store: Arc<StateStore>,
        genesis: Hash,
        algo: PowAlgorithm,
        initial_bits: u32,
        storage: StoragePolicy,
    ) -> Result<Self, AdmitError> {
        let mut boot = ChainBootConfig::default();
        boot.pow = algo;
        boot.initial_bits = initial_bits;
        boot.daa.min_level = if initial_bits == 0 {
            0
        } else {
            boot.daa.min_level.max(1)
        };
        Self::bootstrap_with(store, genesis, boot, storage)
    }

    pub fn bootstrap_with(
        store: Arc<StateStore>,
        genesis: Hash,
        boot: ChainBootConfig,
        storage: StoragePolicy,
    ) -> Result<Self, AdmitError> {
        let (dag, ghostdag) =
            rebuild_dag_from_store(store.as_ref(), genesis, boot.ghostdag.clone())?;

        let difficulty = load_or_init_difficulty(&store, boot.initial_bits, boot.daa.min_level)?;
        let auth = if boot.chain_id.is_empty() {
            None
        } else {
            Some(TxAuthContext {
                chain_id: boot.chain_id,
                genesis,
            })
        };

        let mut chain = Self {
            store,
            genesis,
            dag,
            ghostdag,
            pow: LeadingZeroPow::new(boot.pow),
            emission: boot.emission,
            daa: boot.daa,
            difficulty,
            storage,
            limits: ConsensusLimits::default(),
            auth,
            consensus_policy_hash: boot.consensus_policy_hash,
        };
        // Fresh / upgraded datadirs: ensure virtual tip meta exists.
        if chain.load_virtual_tip()?.is_none() {
            let tip = chain.select_virtual_tip()?.unwrap_or(genesis);
            chain.persist_virtual_tip(tip)?;
        }
        // Crash recovery: finish any in-flight virtual reorg before serving.
        chain.recover_pending_virtual()?;
        // Repair pre-PR-79 journals that persisted subsidy=0 so reorg accounting matches.
        chain.migrate_legacy_journal_subsidies()?;
        Ok(chain)
    }

    #[allow(dead_code)]
    pub fn genesis(&self) -> Hash {
        self.genesis
    }

    pub fn virtual_tip(&self) -> Result<Hash, AdmitError> {
        Ok(self.load_virtual_tip()?.unwrap_or(self.genesis))
    }

    pub fn pow_algorithm(&self) -> PowAlgorithm {
        self.pow.algorithm()
    }

    pub fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    pub fn storage_policy(&self) -> StoragePolicy {
        self.storage
    }

    /// Confirmations for a block that is **blue in the current virtual tip**.
    ///
    /// Returns `None` when the block is unknown, red/non-accepted in the virtual view,
    /// or has no UTXO journal (never applied). Depth is
    /// `virtual_blue_score − block_blue_score + 1`.
    pub fn confirmations(&self, block_id: &Hash) -> Option<u64> {
        let tip = self.virtual_tip().ok()?;
        if !self.ghostdag.is_blue_in_view(&tip, block_id) {
            return None;
        }
        if *block_id != self.genesis
            && load_utxo_journal(self.store.as_ref(), block_id)
                .ok()
                .flatten()
                .is_none()
        {
            return None;
        }
        let block_score = self.ghostdag.blue_score(block_id)?;
        let tip_score = self.ghostdag.blue_score(&tip).unwrap_or(block_score);
        Some(tip_score.saturating_sub(block_score).saturating_add(1))
    }

    pub fn tips(&self) -> Result<Vec<Hash>, AdmitError> {
        let bytes = self
            .store
            .get_cf(ColumnFamily::Meta, meta_keys::TIPS)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
            .unwrap_or_default();
        if bytes.is_empty() {
            return Ok(self.dag.tips());
        }
        borsh::from_slice(&bytes).map_err(|e| AdmitError::Storage(e.to_string()))
    }

    /// Bitcoin-style block locator along the virtual selected-parent spine (newest → genesis).
    pub fn block_locator(&self) -> Result<Vec<Hash>, AdmitError> {
        use agora_p2p::MAX_LOCATOR_HASHES;

        let tip = self.virtual_tip()?;
        let mut hashes = Vec::with_capacity(MAX_LOCATOR_HASHES);
        let mut hash = tip;
        let mut step: u64 = 1;
        let mut index: u64 = 0;
        loop {
            hashes.push(hash);
            if hash == self.genesis || hashes.len() >= MAX_LOCATOR_HASHES.saturating_sub(1) {
                break;
            }
            // Walk `step` selected parents toward genesis.
            for _ in 0..step {
                match self.ghostdag.selected_parent(&hash) {
                    Some(parent) => hash = parent,
                    None => {
                        hash = self.genesis;
                        break;
                    }
                }
                if hash == self.genesis {
                    break;
                }
            }
            index = index.saturating_add(1);
            if index >= 10 {
                step = step.saturating_mul(2).max(1);
            }
        }
        if hashes.last().copied() != Some(self.genesis) {
            hashes.push(self.genesis);
        }
        Ok(hashes)
    }

    /// Headers on the selected-parent path from the first known locator hash toward virtual tip.
    ///
    /// Returns oldest → newest, excluding the common ancestor, capped by `limit`.
    pub fn headers_after_locator(
        &self,
        locator: &[Hash],
        limit: u32,
        stop_hash: Option<Hash>,
    ) -> Result<Vec<agora_types::BlockHeader>, AdmitError> {
        use agora_p2p::{MAX_HEADERS_PER_RESPONSE, MAX_LOCATOR_HASHES};

        let limit = (limit.max(1)).min(MAX_HEADERS_PER_RESPONSE) as usize;
        let locator = if locator.len() > MAX_LOCATOR_HASHES {
            &locator[..MAX_LOCATOR_HASHES]
        } else {
            locator
        };

        let mut ancestor = None;
        for hash in locator {
            if self.has_header(hash)? {
                ancestor = Some(*hash);
                break;
            }
        }
        let ancestor = ancestor.unwrap_or(self.genesis);
        if !self.has_header(&ancestor)? {
            return Ok(Vec::new());
        }

        let tip = self.virtual_tip()?;
        if tip == ancestor {
            return Ok(Vec::new());
        }

        // Walk tip → ancestor via durable headers (bodies may be pruned).
        let mut newest_first = Vec::new();
        let mut cursor = tip;
        loop {
            if cursor == ancestor {
                break;
            }
            let header = self.load_header(&cursor)?.ok_or_else(|| {
                AdmitError::Storage(format!("missing header {}", cursor.to_hex()))
            })?;
            newest_first.push(header);
            match self.ghostdag.selected_parent(&cursor) {
                Some(parent) => cursor = parent,
                None => return Ok(Vec::new()), // exhausted without shared ancestor
            }
            if newest_first.len() > 1_000_000 {
                return Err(AdmitError::Storage("headers path too long".into()));
            }
        }

        newest_first.reverse();
        if let Some(stop) = stop_hash {
            if let Some(pos) = newest_first.iter().position(|h| h.hash() == stop) {
                newest_first.truncate(pos + 1);
            }
        }
        newest_first.truncate(limit);
        Ok(newest_first)
    }

    pub fn load_block(&self, hash: &Hash) -> Result<Option<Block>, AdmitError> {
        load_block_bytes(self.store.as_ref(), hash)
    }

    /// Load a durable header (Warm `header/*`), backfilling from a full body if needed.
    pub fn load_header(&self, hash: &Hash) -> Result<Option<BlockHeader>, AdmitError> {
        if let Some(header) = load_header(self.store.as_ref(), hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        {
            return Ok(Some(header));
        }
        if let Some(block) = self.load_block(hash)? {
            let _ = store_header(self.store.as_ref(), hash, &block.header);
            return Ok(Some(block.header));
        }
        Ok(None)
    }

    pub fn has_block(&self, hash: &Hash) -> Result<bool, AdmitError> {
        Ok(self.load_block(hash)?.is_some() || self.dag.contains(hash))
    }

    pub fn has_header(&self, hash: &Hash) -> Result<bool, AdmitError> {
        Ok(self.load_header(hash)?.is_some() || self.dag.contains(hash))
    }

    /// Parents of `block` that are not yet in the local DAG / store.
    pub fn missing_parents_of(&self, block: &Block) -> Vec<Hash> {
        block
            .header
            .parents
            .iter()
            .copied()
            .filter(|p| !self.dag.contains(p))
            .collect()
    }

    /// Build a mining template parented to current tips with coinbase + transfers.
    ///
    /// Coinbase value is emission ([`EmissionSchedule::reward_at_blue_score`]) plus
    /// the sum of transfer fees (`in − out`) so miners collect relay fees.
    /// `tx_root` commits to coinbase followed by `transfers` so PoW binds the full body.
    pub fn block_template(
        &self,
        payout: Address,
        transfers: &[Transaction],
    ) -> Result<Block, AdmitError> {
        self.block_template_lanes(payout, transfers, &[], &[], &[])
    }

    /// Build a mining template with Trident body lanes.
    pub fn block_template_lanes(
        &self,
        payout: Address,
        transfers: &[Transaction],
        account_transfers: &[agora_types::AccountTransfer],
        stake_ops: &[agora_types::SignedStakeTx],
        ovl_executions: &[agora_types::OvlExecutionTx],
    ) -> Result<Block, AdmitError> {
        let parents = self.select_template_parents()?;
        let timestamp_ms = self.template_timestamp_ms(&parents)?;
        let bits = self.expected_bits_for_parents(&parents)?;
        // Simulate candidate GHOSTDAG so subsidy / RandomX epoch match admission.
        let blue_score = self.simulate_blue_score(&parents, bits)?;
        let scheduled = self.emission.reward_at_blue_score(blue_score);
        let emission = self.clamp_emission(scheduled)?;
        let max_transfers = self.limits.max_block_transactions.saturating_sub(1);
        let included = &transfers[..transfers.len().min(max_transfers)];
        // Fee total must match only the transfers that enter the block body.
        let fees = sum_transfer_fees(self.store.as_ref(), included)
            .map_err(|e| AdmitError::Utxo(e.to_string()))?;
        let reward = emission
            .checked_add(fees)
            .ok_or_else(|| AdmitError::Utxo("coinbase reward overflow".into()))?;
        // Parent-set + timestamp commitment in the low 32 bits; high 32 = miner entropy
        // so sibling templates at the same tip time still get unique coinbase txids.
        let extranonce = template_extranonce(timestamp_ms);
        let coinbase_nonce = coinbase_commitment_nonce(&parents, timestamp_ms, extranonce);
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(reward),
                address: payout,
            }],
            coinbase_nonce,
        );
        let mut transactions = Vec::with_capacity(1 + included.len());
        transactions.push(coinbase);
        transactions.extend(included.iter().cloned());
        let mut block = Block {
            header: BlockHeader {
                version: 1,
                parents,
                timestamp_ms,
                bits,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions,
            account_transfers: account_transfers.to_vec(),
            stake_ops: stake_ops.to_vec(),
            ovl_executions: ovl_executions.to_vec(),
        };
        block.header.tx_root = block.compute_body_root();
        Ok(block)
    }

    /// Template time: `max(local_now, max_parent_ts + 1, MTP + 1)`.
    fn template_timestamp_ms(&self, parents: &[Hash]) -> Result<u64, AdmitError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut parent_max = 0u64;
        for parent in parents {
            if let Some(header) = self.load_header(parent)? {
                parent_max = parent_max.max(header.timestamp_ms);
            }
        }
        let mut mtp = 0u64;
        if let Some(sp) = self.ghostdag.select_parent_among(parents) {
            let mut spine_ts = Vec::new();
            let mut cursor = Some(sp);
            while let Some(hash) = cursor {
                if spine_ts.len() >= 11 {
                    break;
                }
                if let Some(header) = self.load_header(&hash)? {
                    spine_ts.push(header.timestamp_ms);
                }
                cursor = self.ghostdag.selected_parent(&hash);
            }
            mtp = median_time_past(&spine_ts);
        }
        Ok(now_ms
            .max(parent_max.saturating_add(1))
            .max(mtp.saturating_add(1)))
    }

    /// Tips for mining: same work-then-score ranking as virtual tip selection.
    ///
    /// The current virtual tip is always placed first when it is among the tips, so
    /// parent truncation under tip floods cannot drop the consensus tip.
    fn select_template_parents(&self) -> Result<Vec<Hash>, AdmitError> {
        let mut tips = self.tips()?;
        tips.sort_by(|a, b| {
            let wa = self.ghostdag.blue_work(a).unwrap_or(0);
            let wb = self.ghostdag.blue_work(b).unwrap_or(0);
            let sa = self.ghostdag.blue_score(a).unwrap_or(0);
            let sb = self.ghostdag.blue_score(b).unwrap_or(0);
            wb.cmp(&wa)
                .then_with(|| sb.cmp(&sa))
                .then_with(|| b.as_bytes().cmp(a.as_bytes()))
        });
        if let Ok(Some(virtual_tip)) = self.select_virtual_tip() {
            if let Some(pos) = tips.iter().position(|h| *h == virtual_tip) {
                let tip = tips.remove(pos);
                tips.insert(0, tip);
            }
        }
        tips.truncate(self.limits.max_block_parents);
        Ok(tips)
    }

    /// RandomX epoch for a candidate parented at `parents` (blue-score anchored).
    pub fn randomx_epoch_for_parents(&self, parents: &[Hash]) -> u64 {
        let bits = self
            .expected_bits_for_parents(parents)
            .unwrap_or_else(|_| self.difficulty.as_bits());
        let score = self
            .simulate_blue_score(parents, bits)
            .unwrap_or_else(|_| self.estimate_blue_score(parents));
        RandomXPowHasher::epoch_from_blue_score(score)
    }

    /// Lower-bound estimate: `max(parent blue_score) + 1` (ignores mergeset blues).
    pub fn estimate_blue_score(&self, parents: &[Hash]) -> u64 {
        if parents.is_empty() {
            return 1;
        }
        parents
            .iter()
            .filter_map(|p| self.ghostdag.blue_score(p))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    /// Simulate GHOSTDAG coloring for a hypothetical candidate with `parents`.
    ///
    /// Used by the template so coinbase subsidy and RandomX epoch match the score
    /// admission will assign (which can exceed `max(parent)+1` when mergeset blues
    /// are accepted).
    pub fn simulate_blue_score(&self, parents: &[Hash], bits: u32) -> Result<u64, AdmitError> {
        if parents.is_empty() {
            return Ok(1);
        }
        for parent in parents {
            if !self.dag.contains(parent) {
                return Err(AdmitError::MissingParent(*parent));
            }
        }
        let mut dag = self.dag.clone();
        let mut ghost = self.ghostdag.clone();
        let placeholder = Hash::hash_borsh(&(b"agora-template-preview-v1", parents, bits));
        dag.insert(placeholder, parents.to_vec())
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        let data = ghost
            .add_block_with_work(&dag, placeholder, work_from_bits(bits))
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        Ok(data.blue_score)
    }

    /// Walk the selected-parent spine collecting DAA samples (oldest → newest).
    ///
    /// Prefers durable headers so pruned bodies still retarget correctly.
    fn daa_window(&self, tip: Hash) -> Result<Vec<DaaSample>, AdmitError> {
        let want = (self.daa.window_size as usize).saturating_add(1);
        let mut newest_first = Vec::with_capacity(want.min(64));
        let mut cursor = Some(tip);
        while let Some(hash) = cursor {
            if newest_first.len() >= want {
                break;
            }
            if let Some(header) = self.load_header(&hash)? {
                let block_work = work_from_bits(header.bits);
                let blue_work = self.ghostdag.blue_work(&hash).unwrap_or(block_work);
                newest_first.push(DaaSample {
                    timestamp_ms: header.timestamp_ms,
                    blue_work,
                    block_work,
                });
            }
            cursor = self.ghostdag.selected_parent(&hash);
        }
        newest_first.reverse();
        Ok(newest_first)
    }

    /// Expected `header.bits` for a candidate parented at `parents`.
    ///
    /// Derived solely from the canonical selected parent's DAA window — never from the
    /// node-local cached difficulty (which is only a mining-tip hint).
    pub fn expected_bits_for_parents(&self, parents: &[Hash]) -> Result<u32, AdmitError> {
        let Some(sp) = self.ghostdag.select_parent_among(parents) else {
            return Ok(self.daa.min_level);
        };
        let parent_bits = self
            .load_header(&sp)?
            .map(|h| Difficulty::new(h.bits))
            .unwrap_or_else(|| Difficulty::new(self.daa.min_level));
        let window = self.daa_window(sp)?;
        Ok(next_difficulty_weighted(&self.daa, parent_bits, &window).as_bits())
    }

    fn persist_difficulty(&self) -> Result<(), AdmitError> {
        self.store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::DAA_DIFFICULTY,
                &self.difficulty.level.to_le_bytes(),
            )
            .map_err(|e| AdmitError::Storage(e.to_string()))
    }

    /// Verify PoW + parent-contextual DAA, validate txs against a non-mutating UTXO
    /// overlay, then atomically persist DAG state and reorg virtual UTXO.
    pub fn admit_block(&mut self, block: Block) -> Result<Hash, AdmitError> {
        let id = block.id();
        if self.dag.contains(&id) {
            return Err(AdmitError::Duplicate(id.to_hex()));
        }

        self.check_parents(&block)?;
        // Parent-age is relay/DoS policy only — never a consensus reject (see
        // `parent_age_ok_for_relay`). Lagging nodes must admit the same DAG.
        self.check_size_limits(&block)?;
        self.check_timestamps(&block)?;
        self.check_coinbase_commitment(&block)?;
        self.check_coinbase_maturity(&block)?;

        let expected = self.expected_bits_for_parents(&block.header.parents)?;
        if block.header.bits != expected {
            return Err(AdmitError::WrongDifficulty {
                expected,
                got: block.header.bits,
            });
        }

        if block.header.tx_root != block.compute_body_root() {
            return Err(AdmitError::BadTxRoot);
        }

        self.verify_pow(&block)?;

        // Color in-memory so we can prove blue_order(candidate) before any disk write.
        self.dag
            .insert(id, block.header.parents.clone())
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        if let Err(e) =
            self.ghostdag
                .add_block_with_work(&self.dag, id, work_from_bits(block.header.bits))
        {
            let _ = self.dag.remove_tip(&id);
            return Err(AdmitError::Consensus(e.to_string()));
        }

        // Full blue_order UTXO proof (mergeset blues + candidate) on a non-mutating overlay.
        if let Err(err) = self.validate_blue_order_utxo(&block, id) {
            self.ghostdag.remove(&id);
            let _ = self.dag.remove_tip(&id);
            return Err(err);
        }

        let old_virtual = self.virtual_tip()?;
        let new_virtual = match self.select_virtual_tip()? {
            Some(tip) => tip,
            None => {
                self.ghostdag.remove(&id);
                let _ = self.dag.remove_tip(&id);
                return Err(AdmitError::Consensus("no virtual tip".into()));
            }
        };

        // Finality frontier: reject tip changes that abandon a finalized blue.
        if let Err(err) = self.guard_reorg_vs_finality(old_virtual, new_virtual) {
            self.ghostdag.remove(&id);
            let _ = self.dag.remove_tip(&id);
            return Err(err);
        }

        // Single batch: body/header/tx-index/tips/ghostdag (+ pending marker).
        if let Err(err) = self.persist_block_atomic(&block, id, new_virtual) {
            self.ghostdag.remove(&id);
            let _ = self.dag.remove_tip(&id);
            return Err(err);
        }

        // UTXO reorg (own crash-recovery via pending_virtual).
        self.reorg_utxo_to_virtual(old_virtual, new_virtual)?;

        // PoW leg of Trident finality for the new virtual tip (PoS may still be pending).
        if let Err(err) = self.note_pow_on_virtual_tip(new_virtual) {
            debug!(error = %err, "finality pow note skipped");
        }

        // Cached mining difficulty tracks the virtual tip only — never a side-block path.
        if new_virtual != old_virtual {
            if let Some(sp) = self.ghostdag.selected_parent(&new_virtual) {
                let parent_bits = self
                    .load_header(&sp)?
                    .map(|h| Difficulty::new(h.bits))
                    .unwrap_or(self.difficulty);
                let window = self.daa_window(sp)?;
                self.difficulty = next_difficulty_weighted(&self.daa, parent_bits, &window);
            } else {
                let bits = self
                    .load_header(&new_virtual)?
                    .map(|h| h.bits)
                    .unwrap_or(self.difficulty.as_bits());
                self.difficulty = Difficulty::new(bits).clamp(&self.daa);
            }
            self.persist_difficulty()?;
        }

        if let Err(err) = self.prune_hot_window() {
            debug!(error = %err, "hot prune skipped");
        }

        Ok(id)
    }

    fn verify_pow(&self, block: &Block) -> Result<(), AdmitError> {
        let digest = match self.pow.algorithm() {
            PowAlgorithm::RandomX => {
                let epoch = self.randomx_epoch_for_parents(&block.header.parents);
                RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch)
            }
            PowAlgorithm::KHeavyHash => KHeavyHashPowHasher.pow_hash(&block.header),
        };
        if LeadingZeroPow::leading_zero_bits(&digest) < block.header.bits {
            return Err(AdmitError::InvalidPow);
        }
        Ok(())
    }

    /// Relay/DoS helper: true when every parent is within `max_parent_blue_score_lag`
    /// of this node's virtual tip. **Not** a consensus rule — gossip may drop stale
    /// candidates, but admission must still accept them for DAG convergence.
    pub fn parent_age_ok_for_relay(&self, block: &Block) -> bool {
        let tip = match self.virtual_tip() {
            Ok(t) => t,
            Err(_) => return true,
        };
        let tip_score = self.ghostdag.blue_score(&tip).unwrap_or(0);
        let lag = self.limits.max_parent_blue_score_lag;
        for parent in &block.header.parents {
            let score = self.ghostdag.blue_score(parent).unwrap_or(0);
            if tip_score.saturating_sub(score) > lag {
                return false;
            }
        }
        true
    }

    /// Coinbase nonce must commit to parent set + timestamp (low 32 bits); high 32
    /// bits are miner entropy so sibling blocks remain unique.
    fn check_coinbase_commitment(&self, block: &Block) -> Result<(), AdmitError> {
        let coinbases: Vec<_> = block
            .transactions
            .iter()
            .filter(|tx| tx.inputs.is_empty())
            .collect();
        if coinbases.len() != 1 {
            return Err(AdmitError::Utxo(format!(
                "expected exactly one coinbase, got {}",
                coinbases.len()
            )));
        }
        let cb = coinbases[0];
        let expected_low =
            coinbase_commitment_low(&block.header.parents, block.header.timestamp_ms);
        let got_low = cb.nonce as u32;
        if got_low != expected_low {
            return Err(AdmitError::Utxo(format!(
                "coinbase nonce low bits {got_low:#x} must commit to parents+timestamp ({expected_low:#x})"
            )));
        }
        Ok(())
    }

    /// Prove `blue_order(candidate)` is a valid virtual UTXO transition before durable writes.
    ///
    /// Applies every newly accepted blue (including merge-set siblings) then the
    /// candidate on a copy-on-write overlay over the live store. Duplicate / conflicting
    /// transfers are soft-accepted (first blue in order wins).
    fn validate_blue_order_utxo(&self, block: &Block, id: Hash) -> Result<(), AdmitError> {
        let blues = self
            .ghostdag
            .blue_order(&self.dag, id)
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        let current = self.virtual_tip()?;
        let applied = self.applied_blues(current)?;
        let prefix = common_prefix_len(&applied, &blues);

        let overlay = self.clone_utxo_overlay()?;
        let mut issued = self.load_issued_supply()?;
        for hash in applied[prefix..].iter().rev() {
            if *hash == self.genesis {
                continue;
            }
            if let Some(journal) =
                load_utxo_journal(&overlay, hash).map_err(|e| AdmitError::Storage(e.to_string()))?
            {
                // Match live reorg: subtract reverted subsidies before applying the target.
                issued = issued.saturating_sub(journal.subsidy);
                let batch = revert_journal_batched(&journal)
                    .map_err(|e| AdmitError::Utxo(e.to_string()))?;
                overlay
                    .write_batch(batch)
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
                overlay
                    .delete_cf(ColumnFamily::Warm, &utxo_diff_key(hash))
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
                overlay
                    .delete_cf(
                        ColumnFamily::Warm,
                        &agora_state_machine::acceptance_key(hash),
                    )
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
            }
        }

        let max = self.load_max_supply()?;
        for hash in &blues[prefix..] {
            if *hash == self.genesis {
                continue;
            }
            let body = if *hash == id {
                block.clone()
            } else {
                self.load_block(hash)?.ok_or_else(|| {
                    AdmitError::Storage(format!(
                        "missing body for blue {} (pruned node cannot validate this merge; use archival or raise hot_window)",
                        hash.to_hex()
                    ))
                })?
            };
            let blue_score = self.ghostdag.blue_score(hash).unwrap_or(1);
            let scheduled = self.emission.reward_at_blue_score(blue_score);
            let emission = scheduled.min(max.saturating_sub(issued.min(max)));
            let mut applied =
                apply_block_batched_virtual(&overlay, &body, emission, self.auth.as_ref())
                    .map_err(|e| AdmitError::Utxo(e.to_string()))?;
            if issued.saturating_add(applied.journal.subsidy) > max {
                return Err(AdmitError::SupplyCapExceeded);
            }
            issued = issued.saturating_add(applied.journal.subsidy);
            let bytes =
                borsh::to_vec(&applied.journal).map_err(|e| AdmitError::Storage(e.to_string()))?;
            applied
                .batch
                .put_cf(ColumnFamily::Warm, &utxo_diff_key(hash), &bytes);
            applied.acceptance.block_hash = *hash;
            agora_state_machine::put_acceptance_into(&mut applied.batch, hash, &applied.acceptance)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
            overlay
                .write_batch(applied.batch)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    fn clone_utxo_overlay(&self) -> Result<StateStore, AdmitError> {
        Ok(StateStore::open_cow_overlay(self.store.clone()))
    }

    fn check_parents(&self, block: &Block) -> Result<(), AdmitError> {
        let parents = &block.header.parents;
        if parents.is_empty() {
            return Err(AdmitError::TooFewParents);
        }
        if parents.len() > self.limits.max_block_parents {
            return Err(AdmitError::TooManyParents {
                got: parents.len(),
                max: self.limits.max_block_parents,
            });
        }
        let mut seen = HashSet::new();
        for parent in parents {
            if !seen.insert(*parent) {
                return Err(AdmitError::DuplicateParent(parent.to_hex()));
            }
            if !self.dag.contains(parent) {
                return Err(AdmitError::MissingParent(*parent));
            }
        }
        Ok(())
    }

    fn check_size_limits(&self, block: &Block) -> Result<(), AdmitError> {
        if block.transactions.len() > self.limits.max_block_transactions {
            return Err(AdmitError::TooManyTransactions {
                got: block.transactions.len(),
                max: self.limits.max_block_transactions,
            });
        }
        let block_bytes = borsh::to_vec(block).map_err(|e| AdmitError::Storage(e.to_string()))?;
        if block_bytes.len() > self.limits.max_block_bytes {
            return Err(AdmitError::BlockTooLarge {
                got: block_bytes.len(),
                max: self.limits.max_block_bytes,
            });
        }
        for (tx_index, tx) in block.transactions.iter().enumerate() {
            if tx.inputs.len() > self.limits.max_tx_inputs {
                return Err(AdmitError::TooManyTxInputs {
                    tx_index,
                    got: tx.inputs.len(),
                    max: self.limits.max_tx_inputs,
                });
            }
            if tx.outputs.len() > self.limits.max_tx_outputs {
                return Err(AdmitError::TooManyTxOutputs {
                    tx_index,
                    got: tx.outputs.len(),
                    max: self.limits.max_tx_outputs,
                });
            }
            let tx_bytes = borsh::to_vec(tx).map_err(|e| AdmitError::Storage(e.to_string()))?;
            if tx_bytes.len() > self.limits.max_tx_bytes {
                return Err(AdmitError::TxTooLarge {
                    tx_index,
                    got: tx_bytes.len(),
                    max: self.limits.max_tx_bytes,
                });
            }
        }
        Ok(())
    }

    fn check_timestamps(&self, block: &Block) -> Result<(), AdmitError> {
        let mut parent_ts: Vec<u64> = Vec::new();
        let mut parent_max = 0u64;
        for parent in &block.header.parents {
            if let Some(header) = self.load_header(parent)? {
                parent_max = parent_max.max(header.timestamp_ms);
                parent_ts.push(header.timestamp_ms);
            }
        }
        // Strictly after max parent timestamp (equal stamps rejected).
        if block.header.timestamp_ms <= parent_max {
            return Err(AdmitError::TimestampBeforeParent {
                ts: block.header.timestamp_ms,
                parent_ts: parent_max,
            });
        }
        // Median-time-past over selected-parent spine (up to 11).
        if let Some(sp) = self.ghostdag.select_parent_among(&block.header.parents) {
            let mut spine_ts = Vec::new();
            let mut cursor = Some(sp);
            while let Some(hash) = cursor {
                if spine_ts.len() >= 11 {
                    break;
                }
                if let Some(header) = self.load_header(&hash)? {
                    spine_ts.push(header.timestamp_ms);
                }
                cursor = self.ghostdag.selected_parent(&hash);
            }
            let mtp = median_time_past(&spine_ts);
            if block.header.timestamp_ms <= mtp {
                return Err(AdmitError::TimestampBeforeParent {
                    ts: block.header.timestamp_ms,
                    parent_ts: mtp,
                });
            }
        }
        let _ = parent_ts;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let max_ts = now_ms.saturating_add(self.limits.max_timestamp_ahead_ms);
        if block.header.timestamp_ms > max_ts {
            return Err(AdmitError::TimestampTooFarAhead {
                ts: block.header.timestamp_ms,
                max: max_ts,
            });
        }
        Ok(())
    }

    /// Non-genesis coinbase outs need `coinbase_maturity` blue-score depth.
    ///
    /// Resolves the creating block from the UTXO journal that actually created the
    /// live outpoint (not the explorer primary tx index), so multi-inclusion and
    /// identical sibling coinbase txids cannot point maturity at the wrong block.
    fn check_coinbase_maturity(&self, block: &Block) -> Result<(), AdmitError> {
        let bits = self
            .expected_bits_for_parents(&block.header.parents)
            .unwrap_or_else(|_| block.header.bits);
        let next_score = self
            .simulate_blue_score(&block.header.parents, bits)
            .unwrap_or_else(|_| self.estimate_blue_score(&block.header.parents));
        for tx in &block.transactions {
            if tx.inputs.is_empty() {
                continue;
            }
            for input in &tx.inputs {
                let op = input.previous_outpoint;
                let Some(block_id) = self.coinbase_creator_of(&op)? else {
                    continue;
                };
                if block_id == self.genesis {
                    continue; // Premine spendable immediately.
                }
                let created_score = self.ghostdag.blue_score(&block_id).unwrap_or(0);
                let required = created_score.saturating_add(self.limits.coinbase_maturity);
                if next_score < required {
                    return Err(AdmitError::ImmatureCoinbase(format!(
                        "{}:{} created_blue={created_score} need={required} next={next_score}",
                        op.tx_id.to_hex(),
                        op.index
                    )));
                }
            }
        }
        Ok(())
    }

    /// Block whose applied journal created `op`, when that creation was a coinbase.
    fn coinbase_creator_of(&self, op: &agora_types::OutPoint) -> Result<Option<Hash>, AdmitError> {
        let inclusions = list_tx_inclusions(self.store.as_ref(), &op.tx_id)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        for (block_id, idx) in inclusions {
            let Some(created) = self.load_block(&block_id)? else {
                continue;
            };
            let Some(created_tx) = created.transactions.get(idx as usize) else {
                continue;
            };
            if !created_tx.inputs.is_empty() {
                continue;
            }
            let Some(journal) = load_utxo_journal(self.store.as_ref(), &block_id)
                .map_err(|e| AdmitError::Storage(e.to_string()))?
            else {
                continue;
            };
            if journal.created.iter().any(|c| c == op) {
                return Ok(Some(block_id));
            }
        }
        // Fallback for legacy journals / missing inclusions: primary pointer.
        if let Some((block_id, idx)) = lookup_tx_location(self.store.as_ref(), &op.tx_id)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        {
            if let Some(created) = self.load_block(&block_id)? {
                if let Some(created_tx) = created.transactions.get(idx as usize) {
                    if created_tx.inputs.is_empty() {
                        return Ok(Some(block_id));
                    }
                }
            }
        }
        Ok(None)
    }

    fn load_max_supply(&self) -> Result<u64, AdmitError> {
        let bytes = self
            .store
            .get_cf(ColumnFamily::Meta, meta_keys::MAX_SUPPLY)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
            .ok_or_else(|| AdmitError::Storage("missing max_supply".into()))?;
        if bytes.len() != 8 {
            return Err(AdmitError::Storage("bad max_supply".into()));
        }
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn load_issued_supply(&self) -> Result<u64, AdmitError> {
        let Some(bytes) = self
            .store
            .get_cf(ColumnFamily::Meta, meta_keys::ISSUED_SUPPLY)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        else {
            // Pre–Phase 29 datadir: treat premine meta as issued baseline.
            let premine = self
                .store
                .get_cf(ColumnFamily::Meta, meta_keys::PREMINE)
                .map_err(|e| AdmitError::Storage(e.to_string()))?
                .unwrap_or_else(|| 0u64.to_le_bytes().to_vec());
            if premine.len() == 8 {
                return Ok(u64::from_le_bytes(premine.try_into().unwrap()));
            }
            return Ok(0);
        };
        if bytes.len() != 8 {
            return Err(AdmitError::Storage("bad issued_supply".into()));
        }
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn clamp_emission(&self, scheduled: u64) -> Result<u64, AdmitError> {
        let max = self.load_max_supply()?;
        let issued = self.load_issued_supply()?;
        Ok(scheduled.min(max.saturating_sub(issued)))
    }

    fn load_virtual_tip(&self) -> Result<Option<Hash>, AdmitError> {
        let Some(bytes) = self
            .store
            .get_cf(ColumnFamily::Meta, meta_keys::VIRTUAL_TIP)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        else {
            return Ok(None);
        };
        if bytes.len() != 32 {
            return Ok(None);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Some(Hash(arr)))
    }

    fn persist_virtual_tip(&self, tip: Hash) -> Result<(), AdmitError> {
        self.store
            .put_cf(ColumnFamily::Meta, meta_keys::VIRTUAL_TIP, tip.as_bytes())
            .map_err(|e| AdmitError::Storage(e.to_string()))
    }

    fn select_virtual_tip(&self) -> Result<Option<Hash>, AdmitError> {
        // Use in-memory DAG tips so a just-inserted (not yet persisted) block can win.
        // Durable `meta/tips` lags until `persist_block_atomic` commits.
        let tips = self.dag.tips();
        if tips.is_empty() {
            let tips = self.tips()?;
            return Ok(self.ghostdag.select_virtual_tip(&tips));
        }
        Ok(self.ghostdag.select_virtual_tip(&tips))
    }

    /// Blues of `order_past(tip)` in apply order (genesis first).
    fn applied_blues(&self, tip: Hash) -> Result<Vec<Hash>, AdmitError> {
        self.ghostdag
            .blue_order(&self.dag, tip)
            .map_err(|e| AdmitError::Consensus(e.to_string()))
    }

    /// Sync live UTXO from blues(`old`) → blues(`new`) via durable journals.
    ///
    /// Crash safety: writes `meta/pending_virtual` before mutating UTXO, commits all
    /// unapplies in one [`WriteBatch`], applies each new blue atomically, then commits
    /// `virtual_tip` + clears pending in a final batch. [`Self::recover_pending_virtual`]
    /// finishes an interrupted reorg on bootstrap.
    fn reorg_utxo_to_virtual(&mut self, old: Hash, new: Hash) -> Result<(), AdmitError> {
        if old == new {
            let mut tip_batch = WriteBatch::new();
            tip_batch.put_cf(ColumnFamily::Meta, meta_keys::VIRTUAL_TIP, new.as_bytes());
            tip_batch.delete_cf(ColumnFamily::Meta, meta_keys::PENDING_VIRTUAL);
            self.store
                .write_batch(tip_batch)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
            return Ok(());
        }

        // Durably mark the target tip before any UTXO mutation.
        self.store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::PENDING_VIRTUAL,
                new.as_bytes(),
            )
            .map_err(|e| AdmitError::Storage(e.to_string()))?;

        let applied = self.applied_blues(old)?;
        let target = self.applied_blues(new)?;
        let prefix = common_prefix_len(&applied, &target);

        // All unapplies share one batch (ops ordered reverse along the abandoned suffix).
        let mut unapply_batch = WriteBatch::new();
        let mut issued = self.load_issued_supply()?;
        for hash in applied[prefix..].iter().rev() {
            if let Some(subsidy) = self.plan_unapply_block(*hash, &mut unapply_batch)? {
                issued = issued.saturating_sub(subsidy);
            }
        }
        if !unapply_batch.is_empty() {
            unapply_batch.put_cf(
                ColumnFamily::Meta,
                meta_keys::ISSUED_SUPPLY,
                &issued.to_le_bytes(),
            );
            self.store
                .write_batch(unapply_batch)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
        }

        for (offset, hash) in target[prefix..].iter().enumerate() {
            if let Err(err) = self.apply_block_to_virtual(*hash) {
                // Best-effort restore toward `old`. Only clear PENDING_VIRTUAL when
                // restoration fully succeeds — otherwise leave the marker for recovery.
                let restored = self.try_restore_after_failed_reorg(
                    old,
                    &applied[prefix..],
                    &target[prefix..prefix + offset],
                );
                if restored {
                    let mut clear = WriteBatch::new();
                    clear.put_cf(ColumnFamily::Meta, meta_keys::VIRTUAL_TIP, old.as_bytes());
                    clear.delete_cf(ColumnFamily::Meta, meta_keys::PENDING_VIRTUAL);
                    let _ = self.store.write_batch(clear);
                } else {
                    warn!(
                        error = %err,
                        "reorg apply failed and restore incomplete; pending_virtual retained"
                    );
                }
                return Err(err);
            }
        }

        // Final tip meta + clear pending + refresh primary tx pointers.
        let mut tip_batch = WriteBatch::new();
        tip_batch.put_cf(ColumnFamily::Meta, meta_keys::VIRTUAL_TIP, new.as_bytes());
        tip_batch.delete_cf(ColumnFamily::Meta, meta_keys::PENDING_VIRTUAL);
        for hash in applied[prefix..].iter() {
            if *hash == self.genesis {
                continue;
            }
            // Abandoned blues: repoint primary index away from non-virtual inclusions.
            if let Some(block) = self.load_block(hash)? {
                for tx in &block.transactions {
                    self.repoint_primary_tx(&mut tip_batch, &tx.tx_id(), &target)?;
                }
            }
        }
        for hash in &target[prefix..] {
            if *hash == self.genesis {
                continue;
            }
            if let Some(block) = self.load_block(hash)? {
                for (index, tx) in block.transactions.iter().enumerate() {
                    set_primary_tx_location(&mut tip_batch, &tx.tx_id(), hash, index as u32);
                }
            }
        }
        self.store
            .write_batch(tip_batch)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Undo a partial target suffix and re-apply the abandoned `old` suffix.
    /// Returns true only when every step succeeds.
    fn try_restore_after_failed_reorg(
        &self,
        _old: Hash,
        old_suffix: &[Hash],
        partial_new: &[Hash],
    ) -> bool {
        for undo in partial_new.iter().rev() {
            let mut batch = WriteBatch::new();
            match self.plan_unapply_block(*undo, &mut batch) {
                Ok(Some(sub)) => {
                    let cur = match self.load_issued_supply() {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    batch.put_cf(
                        ColumnFamily::Meta,
                        meta_keys::ISSUED_SUPPLY,
                        &cur.saturating_sub(sub).to_le_bytes(),
                    );
                    if self.store.write_batch(batch).is_err() {
                        return false;
                    }
                }
                Ok(None) => {}
                Err(_) => return false,
            }
        }
        for redo in old_suffix {
            if self.apply_block_to_virtual(*redo).is_err() {
                return false;
            }
        }
        true
    }

    fn repoint_primary_tx(
        &self,
        batch: &mut WriteBatch,
        tx_id: &Hash,
        virtual_blues: &[Hash],
    ) -> Result<(), AdmitError> {
        let inclusions = list_tx_inclusions(self.store.as_ref(), tx_id)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        let blue_set: HashSet<Hash> = virtual_blues.iter().copied().collect();
        if let Some((block_id, index)) = inclusions.into_iter().find(|(b, _)| blue_set.contains(b))
        {
            set_primary_tx_location(batch, tx_id, &block_id, index);
        }
        Ok(())
    }

    /// Finish an interrupted reorg after crash/restart.
    fn recover_pending_virtual(&mut self) -> Result<(), AdmitError> {
        let Some(bytes) = self
            .store
            .get_cf(ColumnFamily::Meta, meta_keys::PENDING_VIRTUAL)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        else {
            return Ok(());
        };
        if bytes.len() != 32 {
            self.store
                .delete_cf(ColumnFamily::Meta, meta_keys::PENDING_VIRTUAL)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
            return Ok(());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let pending = Hash(arr);
        let current = self.virtual_tip()?;
        debug!(
            current = %current.to_hex(),
            pending = %pending.to_hex(),
            "recovering pending virtual reorg"
        );
        self.reorg_utxo_to_virtual(current, pending)
    }

    fn apply_block_to_virtual(&self, hash: Hash) -> Result<(), AdmitError> {
        // Genesis premine is the UTXO baseline (written at ignite); never re-apply.
        if hash == self.genesis {
            return Ok(());
        }
        if load_utxo_journal(self.store.as_ref(), &hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
            .is_some()
        {
            // Already applied (idempotent).
            return Ok(());
        }
        let block = self
            .load_block(&hash)?
            .ok_or_else(|| AdmitError::Storage(format!("missing block {}", hash.to_hex())))?;
        let blue_score = self
            .ghostdag
            .blue_score(&hash)
            .ok_or_else(|| AdmitError::Consensus(format!("uncolored {}", hash.to_hex())))?;
        let scheduled = self.emission.reward_at_blue_score(blue_score);
        let emission = self.clamp_emission(scheduled)?;
        // Atomic commit: UTXO changes + revert journal + issued-supply update land as a
        // single WriteBatch so a crash cannot leave UTXOs and supply out of sync.
        let mut applied =
            apply_block_batched_virtual(self.store.as_ref(), &block, emission, self.auth.as_ref())
                .map_err(|e| AdmitError::Utxo(e.to_string()))?;
        if applied.journal.subsidy > emission {
            return Err(AdmitError::Utxo(format!(
                "coinbase subsidy {} exceeds clamped emission {emission}",
                applied.journal.subsidy
            )));
        }
        let max = self.load_max_supply()?;
        let issued = self.load_issued_supply()?;
        if issued.saturating_add(applied.journal.subsidy) > max {
            return Err(AdmitError::SupplyCapExceeded);
        }
        let subsidy = applied.journal.subsidy;
        let journal_bytes =
            borsh::to_vec(&applied.journal).map_err(|e| AdmitError::Storage(e.to_string()))?;
        applied
            .batch
            .put_cf(ColumnFamily::Warm, &utxo_diff_key(&hash), &journal_bytes);
        applied.acceptance.block_hash = hash;
        agora_state_machine::put_acceptance_into(&mut applied.batch, &hash, &applied.acceptance)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        agora_state_machine::put_issued_supply_into(
            &mut applied.batch,
            agora_types::NativeAssetId::TLT,
            issued.saturating_add(subsidy),
        );
        self.store
            .write_batch(applied.batch)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Plan an unapply into `batch`. Returns subsidy delta when a journal existed.
    /// Missing journal = already unapplied (idempotent, for crash recovery).
    fn plan_unapply_block(
        &self,
        hash: Hash,
        batch: &mut WriteBatch,
    ) -> Result<Option<u64>, AdmitError> {
        if hash == self.genesis {
            return Ok(None);
        }
        let Some(journal) = load_utxo_journal(self.store.as_ref(), &hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        else {
            return Ok(None);
        };
        // Prefer persisted subsidy. Legacy journals (pre-accounting fields deserialize
        // as zeros) leave issued supply unchanged rather than guess package fees.
        let subsidy = journal.subsidy;

        let revert =
            revert_journal_batched(&journal).map_err(|e| AdmitError::Utxo(e.to_string()))?;
        batch.append(revert);
        batch.delete_cf(ColumnFamily::Warm, &utxo_diff_key(&hash));
        agora_state_machine::delete_acceptance_into(batch, &hash);
        Ok(Some(subsidy))
    }

    /// Persist body/header/tx-index/tips/ghostdag (+ pending virtual) in one WriteBatch.
    fn persist_block_atomic(
        &self,
        block: &Block,
        id: Hash,
        pending_virtual: Hash,
    ) -> Result<(), AdmitError> {
        let block_bytes = borsh::to_vec(block).map_err(|e| AdmitError::Storage(e.to_string()))?;
        let mut batch = WriteBatch::new();
        batch.put_cf(ColumnFamily::Hot, id.as_bytes(), &block_bytes);
        if self.storage.archival {
            batch.put_cf(ColumnFamily::Archival, id.as_bytes(), &block_bytes);
        }
        store_header_into(&mut batch, &id, &block.header)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        index_block_transactions_into(&mut batch, block);

        let mut tips = self.tips()?;
        tips.retain(|t| !block.header.parents.iter().any(|p| p == t));
        if !tips.contains(&id) {
            tips.push(id);
        }
        let tips_bytes = borsh::to_vec(&tips).map_err(|e| AdmitError::Storage(e.to_string()))?;
        batch.put_cf(ColumnFamily::Meta, meta_keys::TIPS, &tips_bytes);

        if let Some(snap) = self.ghostdag.snapshot(&id) {
            let record = GhostdagRecord {
                selected_parent: snap.selected_parent,
                blue_score: snap.blue_score,
                blue_work: snap.blue_work,
                block_work: snap.block_work,
                mergeset_blues: snap.mergeset_blues,
            };
            let bytes = borsh::to_vec(&record).map_err(|e| AdmitError::Storage(e.to_string()))?;
            batch.put_cf(ColumnFamily::Warm, &ghostdag_key(&id), &bytes);
        }

        batch.put_cf(
            ColumnFamily::Meta,
            meta_keys::PENDING_VIRTUAL,
            pending_virtual.as_bytes(),
        );

        self.store
            .write_batch(batch)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Drop `cf_hot` bodies beyond [`StoragePolicy::hot_window`] tip-distance.
    ///
    /// Archival copies are required before delete when `archival` is enabled; pruned
    /// nodes (`archival=false`) drop Hot bodies permanently past the window.
    fn prune_hot_window(&self) -> Result<(), AdmitError> {
        let window = self.storage.hot_window;
        if window == 0 {
            return Ok(());
        }

        let tips = self.tips()?;
        let mut keep: HashSet<Hash> = HashSet::new();
        let mut q: VecDeque<(Hash, u32)> = tips.into_iter().map(|t| (t, 0)).collect();
        while let Some((hash, dist)) = q.pop_front() {
            if !keep.insert(hash) {
                continue;
            }
            if dist >= window {
                continue;
            }
            // Prefer durable headers so prune BFS works after bodies are dropped.
            if let Some(header) = self.load_header(&hash)? {
                for parent in header.parents {
                    q.push_back((parent, dist + 1));
                }
            }
        }

        let mut drop: Vec<Hash> = Vec::new();
        self.store
            .for_each_cf(ColumnFamily::Hot, |key, _value| {
                if key.len() != 32 {
                    return Ok(());
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(key);
                let hash = Hash(arr);
                if !keep.contains(&hash) {
                    drop.push(hash);
                }
                Ok(())
            })
            .map_err(|e| AdmitError::Storage(e.to_string()))?;

        let mut pruned = 0u32;
        for hash in drop {
            if self.storage.archival {
                let has_archival = self
                    .store
                    .get_cf(ColumnFamily::Archival, hash.as_bytes())
                    .map_err(|e| AdmitError::Storage(e.to_string()))?
                    .is_some();
                if !has_archival {
                    continue;
                }
            }
            self.store
                .delete_cf(ColumnFamily::Hot, hash.as_bytes())
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
            pruned += 1;
        }
        if pruned > 0 {
            debug!(pruned, keep = keep.len(), window, "pruned hot block bodies");
        }
        Ok(())
    }
}

fn load_block_bytes(store: &StateStore, hash: &Hash) -> Result<Option<Block>, AdmitError> {
    for cf in [ColumnFamily::Hot, ColumnFamily::Archival] {
        if let Some(bytes) = store
            .get_cf(cf, hash.as_bytes())
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        {
            let block =
                borsh::from_slice(&bytes).map_err(|e| AdmitError::Storage(e.to_string()))?;
            return Ok(Some(block));
        }
    }
    // Legacy: some older writes may have placed bodies in Warm under the raw hash key.
    if let Some(bytes) = store
        .get_cf(ColumnFamily::Warm, hash.as_bytes())
        .map_err(|e| AdmitError::Storage(e.to_string()))?
    {
        if let Ok(block) = borsh::from_slice::<Block>(&bytes) {
            return Ok(Some(block));
        }
    }
    Ok(None)
}

fn load_parents_for_rebuild(store: &StateStore, hash: &Hash) -> Result<Vec<Hash>, AdmitError> {
    if let Some(block) = load_block_bytes(store, hash)? {
        let _ = store_header(store, hash, &block.header);
        return Ok(block.header.parents);
    }
    if let Some(header) =
        load_header(store, hash).map_err(|e| AdmitError::Storage(e.to_string()))?
    {
        return Ok(header.parents);
    }
    Err(AdmitError::Storage(format!(
        "missing header/body {} while rebuilding dag",
        hash.to_hex()
    )))
}

fn load_tips_meta(store: &StateStore) -> Result<Vec<Hash>, AdmitError> {
    let bytes = store
        .get_cf(ColumnFamily::Meta, meta_keys::TIPS)
        .map_err(|e| AdmitError::Storage(e.to_string()))?
        .unwrap_or_default();
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    borsh::from_slice(&bytes).map_err(|e| AdmitError::Storage(e.to_string()))
}

/// Rebuild in-memory DAG/GHOSTDAG from durable tips → genesis ancestors.
fn rebuild_dag_from_store(
    store: &StateStore,
    genesis: Hash,
    ghostdag_config: GhostdagConfig,
) -> Result<(Dag, Ghostdag), AdmitError> {
    let mut tips = load_tips_meta(store)?;
    if tips.is_empty() {
        tips.push(genesis);
    }

    let mut pending: HashMap<Hash, Vec<Hash>> = HashMap::new();
    let mut stack = tips;
    while let Some(hash) = stack.pop() {
        if hash == genesis || pending.contains_key(&hash) {
            continue;
        }
        let parents = load_parents_for_rebuild(store, &hash)?;
        for parent in &parents {
            stack.push(*parent);
        }
        pending.insert(hash, parents);
    }

    // Real PoW work per block so rebuilt `blue_work` matches live admission exactly.
    let block_work = |hash: &Hash| -> u128 {
        load_header(store, hash)
            .ok()
            .flatten()
            .map(|h| work_from_bits(h.bits))
            .unwrap_or(1)
    };

    let mut dag = Dag::new();
    dag.insert(genesis, vec![])
        .map_err(|e| AdmitError::Consensus(e.to_string()))?;
    let mut ghostdag = Ghostdag::new(ghostdag_config);
    hydrate_or_recolor(store, &mut ghostdag, &dag, genesis, block_work(&genesis))?;

    while !pending.is_empty() {
        // Canonical (hash-sorted) ready order so restart reconstructs the same DAG
        // regardless of HashMap iteration order. (Coloring is arrival-order independent,
        // but a stable order keeps `blocks_in_insert_order` deterministic too.)
        let mut ready: Vec<Hash> = pending
            .iter()
            .filter(|(_, parents)| parents.iter().all(|p| dag.contains(p)))
            .map(|(hash, _)| *hash)
            .collect();
        if ready.is_empty() {
            return Err(AdmitError::Storage(
                "cannot rebuild dag: missing parents or cycle".into(),
            ));
        }
        ready.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        for hash in ready {
            let parents = pending.remove(&hash).expect("ready hash");
            dag.insert(hash, parents)
                .map_err(|e| AdmitError::Consensus(e.to_string()))?;
            hydrate_or_recolor(store, &mut ghostdag, &dag, hash, block_work(&hash))?;
        }
    }

    Ok((dag, ghostdag))
}

/// Load durable GHOSTDAG coloring when present; otherwise recompute and persist.
fn hydrate_or_recolor(
    store: &StateStore,
    ghostdag: &mut Ghostdag,
    dag: &Dag,
    hash: Hash,
    work: u128,
) -> Result<(), AdmitError> {
    if let Some(record) =
        load_ghostdag_record(store, &hash).map_err(|e| AdmitError::Storage(e.to_string()))?
    {
        ghostdag.import_snapshot(
            hash,
            GhostdagSnapshot {
                selected_parent: record.selected_parent,
                blue_score: record.blue_score,
                blue_work: record.blue_work,
                block_work: record.block_work,
                mergeset_blues: record.mergeset_blues,
            },
        );
        return Ok(());
    }
    ghostdag
        .add_block_with_work(dag, hash, work)
        .map_err(|e| AdmitError::Consensus(e.to_string()))?;
    if let Some(snap) = ghostdag.snapshot(&hash) {
        let record = GhostdagRecord {
            selected_parent: snap.selected_parent,
            blue_score: snap.blue_score,
            blue_work: snap.blue_work,
            block_work: snap.block_work,
            mergeset_blues: snap.mergeset_blues,
        };
        store_ghostdag_record(store, &hash, &record)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
    }
    Ok(())
}

fn common_prefix_len(a: &[Hash], b: &[Hash]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Low 32 bits of the coinbase uniqueness commitment (parents + timestamp).
fn coinbase_commitment_low(parents: &[Hash], timestamp_ms: u64) -> u32 {
    let mut sorted = parents.to_vec();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let tag = Hash::hash_borsh(&(b"agora-cb-parents-v2", sorted));
    let parent_tag = u32::from_le_bytes(tag.as_bytes()[..4].try_into().unwrap());
    parent_tag ^ (timestamp_ms as u32)
}

/// Full coinbase nonce: commitment low bits | miner extranonce high bits.
fn coinbase_commitment_nonce(parents: &[Hash], timestamp_ms: u64, extranonce: u32) -> u64 {
    let low = coinbase_commitment_low(parents, timestamp_ms);
    ((extranonce as u64) << 32) | u64::from(low)
}

fn template_extranonce(timestamp_ms: u64) -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (timestamp_ms as u32)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(nanos)
        .wrapping_add(std::process::id())
}

impl ChainState {
    /// Rewrite legacy journals that loaded with `subsidy = 0` using block bodies.
    ///
    /// Only repairs journals whose `created` set includes the block's coinbase
    /// outpoints (coinbase was applied, accounting fields were not persisted).
    /// Virtual soft-skip journals (duplicate coinbase skipped, transfers may still
    /// create outs) must not be rewritten — that would invent phantom subsidies.
    fn migrate_legacy_journal_subsidies(&self) -> Result<(), AdmitError> {
        let diffs = self
            .store
            .scan_prefix(ColumnFamily::Warm, b"utxo_diff/")
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        for (key, value) in diffs {
            let journal = match agora_state_machine::UtxoJournal::from_bytes(&value) {
                Ok(j) => j,
                Err(_) => continue,
            };
            // Modern journals (including Virtual skip with fees) keep non-zero
            // accounting or empty creates — leave them alone.
            if journal.subsidy != 0
                || journal.coinbase_total != 0
                || journal.fees != 0
                || journal.created.is_empty()
            {
                continue;
            }
            if key.len() < b"utxo_diff/".len() + 32 {
                continue;
            }
            let mut hash_bytes = [0u8; 32];
            hash_bytes.copy_from_slice(&key[b"utxo_diff/".len()..b"utxo_diff/".len() + 32]);
            let hash = Hash(hash_bytes);
            if hash == self.genesis {
                continue;
            }
            let Some(block) = self.load_block(&hash)? else {
                // Pruned body: cannot repair — operator must reindex / reset datadir.
                warn!(
                    block = %hash.to_hex(),
                    "legacy utxo journal needs migration but body is missing"
                );
                continue;
            };
            let Some(cb) = block.transactions.iter().find(|tx| tx.inputs.is_empty()) else {
                continue;
            };
            let cb_txid = cb.tx_id();
            let coinbase_created = journal.created.iter().any(|op| op.tx_id == cb_txid);
            if !coinbase_created {
                // Virtual duplicate-coinbase skip (or transfer-only journal): not legacy.
                continue;
            }
            let mut coinbase_total = 0u64;
            for out in &cb.outputs {
                coinbase_total = coinbase_total.saturating_add(out.value.as_base_units());
            }
            let blue_score = self.ghostdag.blue_score(&hash).unwrap_or(1);
            let emission = self.emission.reward_at_blue_score(blue_score);
            let subsidy = coinbase_total.min(emission);
            let fees = coinbase_total.saturating_sub(subsidy);
            let repaired = agora_state_machine::UtxoJournal {
                spent: journal.spent,
                created: journal.created,
                fees,
                subsidy,
                coinbase_total,
                account_before: journal.account_before,
                stake_meta_before: journal.stake_meta_before,
            };
            let bytes = borsh::to_vec(&repaired).map_err(|e| AdmitError::Storage(e.to_string()))?;
            self.store
                .put_cf(ColumnFamily::Warm, &key, &bytes)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
            debug!(block = %hash.to_hex(), subsidy, fees, "migrated legacy utxo journal");
        }
        Ok(())
    }
}

fn load_or_init_difficulty(
    store: &StateStore,
    initial_bits: u32,
    min_level: u32,
) -> Result<Difficulty, AdmitError> {
    let floor = initial_bits.max(min_level);
    if let Some(bytes) = store
        .get_cf(ColumnFamily::Meta, meta_keys::DAA_DIFFICULTY)
        .map_err(|e| AdmitError::Storage(e.to_string()))?
    {
        if bytes.len() == 4 {
            let level = u32::from_le_bytes(bytes.try_into().unwrap());
            let clamped = level.max(min_level);
            if clamped != level {
                store
                    .put_cf(
                        ColumnFamily::Meta,
                        meta_keys::DAA_DIFFICULTY,
                        &clamped.to_le_bytes(),
                    )
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
            }
            return Ok(Difficulty::new(clamped));
        }
    }
    let difficulty = Difficulty::new(floor);
    store
        .put_cf(
            ColumnFamily::Meta,
            meta_keys::DAA_DIFFICULTY,
            &difficulty.level.to_le_bytes(),
        )
        .map_err(|e| AdmitError::Storage(e.to_string()))?;
    Ok(difficulty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_consensus::RandomXPowHasher;
    use agora_state_machine::GenesisBuilder;

    #[test]
    fn rejects_wrong_bits_and_persists_difficulty() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(chain.difficulty().as_bits(), 0);
        assert_eq!(
            chain
                .block_template(Address::ZERO, &[])
                .unwrap()
                .header
                .bits,
            0
        );

        let mut bad = chain.block_template(Address::ZERO, &[]).unwrap();
        bad.header.bits = 3;
        bad.header.nonce = 1;
        assert!(matches!(
            chain.admit_block(bad),
            Err(AdmitError::WrongDifficulty {
                expected: 0,
                got: 3
            })
        ));

        let mut bad_root = chain.block_template(Address::ZERO, &[]).unwrap();
        bad_root.header.tx_root = Hash::ZERO;
        bad_root.header.nonce = 2;
        assert!(matches!(
            chain.admit_block(bad_root),
            Err(AdmitError::BadTxRoot)
        ));

        // Admit a valid bits=0 coinbase block, then force a retarget from the spine window.
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.nonce = 9;
        block.header.timestamp_ms = 1;
        sync_coinbase_timestamp(&mut block);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        assert_eq!(block.transactions.len(), 1);
        assert!(block.transactions[0].inputs.is_empty());
        let id = chain.admit_block(block).unwrap();

        chain.daa.window_size = 2;
        chain.daa.target_block_time_ms = 10_000;
        chain.difficulty = Difficulty::new(4);
        let window = chain.daa_window(id).unwrap();
        assert!(window.len() >= 2);
        let next = next_difficulty_weighted(&chain.daa, chain.difficulty, &window);
        assert!(next.level > 4);
        chain.difficulty = next;
        chain.persist_difficulty().unwrap();

        let reloaded = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        // Cached mining difficulty is a tip hint that survives restart.
        assert_eq!(reloaded.difficulty().as_bits(), next.level);
        // Template bits are parent-contextual consensus values, not the cache.
        let expected = reloaded
            .expected_bits_for_parents(
                &reloaded
                    .block_template(Address::ZERO, &[])
                    .unwrap()
                    .header
                    .parents,
            )
            .unwrap();
        assert_eq!(
            reloaded
                .block_template(Address::ZERO, &[])
                .unwrap()
                .header
                .bits,
            expected
        );
    }

    #[test]
    fn bootstrap_rebuilds_dag_from_store_tips() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.nonce = 3;
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        let id = chain.admit_block(block).unwrap();

        let reloaded = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert!(reloaded.has_block(&id).unwrap());
        assert!(reloaded.tips().unwrap().contains(&id));
        let child = reloaded.block_template(Address::ZERO, &[]).unwrap();
        assert!(child.header.parents.contains(&id));
    }

    fn sync_coinbase_commitment(block: &mut Block) {
        sync_coinbase_commitment_with(block, 1);
    }

    fn sync_coinbase_commitment_with(block: &mut Block, extranonce: u32) {
        if let Some(cb) = block
            .transactions
            .iter_mut()
            .find(|tx| tx.inputs.is_empty())
        {
            cb.nonce = coinbase_commitment_nonce(
                &block.header.parents,
                block.header.timestamp_ms,
                extranonce,
            );
        }
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
    }

    /// Backward-compatible alias used by older test helpers in this module.
    fn sync_coinbase_timestamp(block: &mut Block) {
        sync_coinbase_commitment(block);
    }

    fn mine_one(chain: &mut ChainState, nonce: u64) -> Hash {
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.nonce = nonce;
        // Space blocks ~at the target so the DAA holds difficulty at the (0) floor;
        // otherwise sub-target spacing correctly raises `bits` and the fixed nonce
        // would no longer satisfy the leading-zero requirement.
        block.header.timestamp_ms = nonce.saturating_mul(1_000);
        sync_coinbase_timestamp(&mut block);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        chain.admit_block(block).unwrap()
    }

    #[test]
    fn hot_window_prunes_old_bodies_keeps_archival() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let policy = StoragePolicy {
            archival: true,
            hot_window: 1,
        };
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0, policy)
                .unwrap();

        let b1 = mine_one(&mut chain, 1);
        let b2 = mine_one(&mut chain, 2);

        // Tip-distance window 1: keep tip + its parents (b2, b1). Genesis should leave Hot.
        assert!(store
            .get_cf(ColumnFamily::Hot, b2.as_bytes())
            .unwrap()
            .is_some());
        assert!(store
            .get_cf(ColumnFamily::Hot, b1.as_bytes())
            .unwrap()
            .is_some());
        assert!(store
            .get_cf(ColumnFamily::Hot, genesis.as_bytes())
            .unwrap()
            .is_none());
        // Archival retains full history.
        assert!(store
            .get_cf(ColumnFamily::Archival, genesis.as_bytes())
            .unwrap()
            .is_some());
        assert!(chain.load_block(&genesis).unwrap().is_some());
    }

    #[test]
    fn pruned_node_skips_archival_writes() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default()
            .with_archival(false)
            .ignite(&store)
            .unwrap();
        assert!(store
            .get_cf(ColumnFamily::Archival, genesis.as_bytes())
            .unwrap()
            .is_none());
        let policy = StoragePolicy {
            archival: false,
            hot_window: 1,
        };
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0, policy)
                .unwrap();
        let b1 = mine_one(&mut chain, 11);
        assert!(store
            .get_cf(ColumnFamily::Archival, b1.as_bytes())
            .unwrap()
            .is_none());
        let _b2 = mine_one(&mut chain, 12);
        // Genesis dropped from Hot past window; without archival it is gone from store bodies.
        assert!(store
            .get_cf(ColumnFamily::Hot, genesis.as_bytes())
            .unwrap()
            .is_none());
        assert!(chain.load_block(&genesis).unwrap().is_none());
        // Durable header remains for IBD / restart.
        assert!(chain.load_header(&genesis).unwrap().is_some());
        // Tips still load.
        assert!(chain.load_block(&_b2).unwrap().is_some());
    }

    #[test]
    fn pruned_node_serves_headers_after_locator() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default()
            .with_archival(false)
            .ignite(&store)
            .unwrap();
        let policy = StoragePolicy {
            archival: false,
            hot_window: 1,
        };
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0, policy)
                .unwrap();
        let mut tip = genesis;
        for nonce in 1..=6u64 {
            tip = mine_child(&mut chain, &[tip], Address::ZERO, nonce);
        }
        assert!(chain.load_block(&genesis).unwrap().is_none());
        assert!(chain.load_header(&genesis).unwrap().is_some());

        let headers = chain.headers_after_locator(&[genesis], 4, None).unwrap();
        assert_eq!(headers.len(), 4);
        assert!(headers[0].parents.contains(&genesis));
        for window in headers.windows(2) {
            assert!(window[1].parents.contains(&window[0].hash()));
        }
        assert_eq!(chain.virtual_tip().unwrap(), tip);
    }

    #[test]
    fn pruned_node_restarts_from_headers() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default()
            .with_archival(false)
            .ignite(&store)
            .unwrap();
        let policy = StoragePolicy {
            archival: false,
            hot_window: 1,
        };
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0, policy)
                .unwrap();
        let mut tip = genesis;
        for nonce in 20..=24u64 {
            tip = mine_child(&mut chain, &[tip], Address::ZERO, nonce);
        }
        assert!(chain.load_block(&genesis).unwrap().is_none());
        drop(chain);

        let reloaded =
            ChainState::bootstrap(store, genesis, PowAlgorithm::RandomX, 0, policy).unwrap();
        assert_eq!(reloaded.virtual_tip().unwrap(), tip);
        assert!(reloaded.has_header(&genesis).unwrap());
        assert!(reloaded.has_header(&tip).unwrap());
        assert!(reloaded.tips().unwrap().contains(&tip));
    }

    #[test]
    fn confirmations_grow_with_blue_score() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(chain.confirmations(&genesis), Some(1));
        let b1 = mine_one(&mut chain, 21);
        let tip = chain.virtual_tip().unwrap();
        eprintln!(
            "b1={} tip={} blue_in={} journal={:?} score={:?}",
            b1.to_hex(),
            tip.to_hex(),
            chain.ghostdag.is_blue_in_view(&tip, &b1),
            load_utxo_journal(chain.store.as_ref(), &b1)
                .unwrap()
                .map(|_| "yes"),
            chain.ghostdag.blue_score(&b1)
        );
        assert_eq!(chain.confirmations(&b1), Some(1));
        assert!(chain.confirmations(&genesis).unwrap() >= 2);
        let b2 = mine_one(&mut chain, 22);
        assert_eq!(chain.confirmations(&b2), Some(1));
        assert!(chain.confirmations(&b1).unwrap() >= 2);
        assert!(chain.confirmations(&genesis).unwrap() >= 3);
    }

    fn mine_child(chain: &mut ChainState, parents: &[Hash], payout: Address, nonce: u64) -> Hash {
        let mut block = chain.block_template(payout, &[]).unwrap();
        block.header.parents = parents.to_vec();
        block.header.bits = chain.expected_bits_for_parents(parents).unwrap();
        block.header.nonce = nonce;
        // Target-spaced timestamps keep the DAA at the difficulty floor (see mine_one).
        // Strictly greater than parent max / MTP — use a monotone clock from nonce.
        block.header.timestamp_ms = nonce.saturating_mul(1_000).max(1);
        let blue_score = chain
            .simulate_blue_score(parents, block.header.bits)
            .unwrap_or_else(|_| chain.estimate_blue_score(parents));
        let emission = chain.emission.reward_at_blue_score(blue_score);
        block.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: payout,
            }],
            coinbase_commitment_nonce(
                &block.header.parents,
                block.header.timestamp_ms,
                nonce as u32,
            ),
        )];
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        chain.admit_block(block).unwrap()
    }

    #[test]
    fn parallel_tip_does_not_mutate_utxo_until_selected() {
        use agora_state_machine::balance_of;

        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(chain.virtual_tip().unwrap(), genesis);

        let miner_a = Address([0xAAu8; 20]);
        let miner_b = Address([0xBBu8; 20]);
        let a = mine_child(&mut chain, &[genesis], miner_a, 100);
        assert_eq!(chain.virtual_tip().unwrap(), a);
        let reward = chain.emission.reward_at_blue_score(2);
        assert_eq!(
            balance_of(store.as_ref(), &miner_a)
                .unwrap()
                .as_base_units(),
            reward
        );

        let b = mine_child(&mut chain, &[genesis], miner_b, 101);
        let virtual_tip = chain.virtual_tip().unwrap();
        assert!(virtual_tip == a || virtual_tip == b);
        // Only the selected tip's coinbase is live; the other remains unapplied.
        let bal_a = balance_of(store.as_ref(), &miner_a)
            .unwrap()
            .as_base_units();
        let bal_b = balance_of(store.as_ref(), &miner_b)
            .unwrap()
            .as_base_units();
        if virtual_tip == a {
            assert_eq!(bal_a, reward);
            assert_eq!(bal_b, 0);
        } else {
            assert_eq!(bal_b, reward);
            assert_eq!(bal_a, 0);
        }

        // Merge tip brings both blues into the virtual past.
        let miner_c = Address([0xCCu8; 20]);
        let c = mine_child(&mut chain, &[a, b], miner_c, 102);
        assert_eq!(chain.virtual_tip().unwrap(), c);
        assert_eq!(
            balance_of(store.as_ref(), &miner_a)
                .unwrap()
                .as_base_units(),
            reward
        );
        assert_eq!(
            balance_of(store.as_ref(), &miner_b)
                .unwrap()
                .as_base_units(),
            reward
        );
        let reward_c = chain
            .emission
            .reward_at_blue_score(chain.ghostdag.blue_score(&c).unwrap());
        assert_eq!(
            balance_of(store.as_ref(), &miner_c)
                .unwrap()
                .as_base_units(),
            reward_c
        );
    }

    #[test]
    fn rejects_too_many_parents_and_bad_timestamp() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let a = mine_child(&mut chain, &[genesis], Address([0xAAu8; 20]), 40);
        let b = mine_child(&mut chain, &[genesis], Address([0xBBu8; 20]), 41);
        chain.limits.max_block_parents = 1;
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.parents = vec![a, b];
        block.header.nonce = 42;
        block.header.timestamp_ms = 42;
        // Rebuild coinbase for emission estimate with these parents.
        let blue_score = chain
            .simulate_blue_score(&[a, b], block.header.bits)
            .unwrap_or_else(|_| chain.estimate_blue_score(&[a, b]));
        let emission = chain.emission.reward_at_blue_score(blue_score);
        block.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: Address::ZERO,
            }],
            coinbase_commitment_nonce(&block.header.parents, block.header.timestamp_ms, 42),
        )];
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        assert!(matches!(
            chain.admit_block(block),
            Err(AdmitError::TooManyParents { .. })
        ));

        chain.limits.max_block_parents = 16;
        let mut early = chain.block_template(Address::ZERO, &[]).unwrap();
        early.header.timestamp_ms = 0;
        early.header.nonce = 43;
        sync_coinbase_commitment_with(&mut early, 43);
        let epoch = chain.randomx_epoch_for_parents(&early.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&early.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= early.header.bits);
        assert!(matches!(
            chain.admit_block(early),
            Err(AdmitError::TimestampBeforeParent { .. })
        ));
    }

    #[test]
    fn rejects_immature_coinbase_with_known_key() {
        use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
        use agora_types::{OutPoint, TxIn};

        const PHRASE: &str =
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let store = Arc::new(StateStore::open_in_memory());
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let miner = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let genesis = GenesisBuilder::default()
            .with_premine_address(Address([9u8; 20]))
            .ignite(&store)
            .unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        chain.limits.coinbase_maturity = 3;
        let mined = mine_child(&mut chain, &[genesis], miner.address(), 60);
        let mined_block = chain.load_block(&mined).unwrap().unwrap();
        let coinbase_txid = mined_block.transactions[0].tx_id();
        let value = mined_block.transactions[0].outputs[0].value;

        let mut spend = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: coinbase_txid,
                    index: 0,
                },
            }],
            vec![TxOut {
                value,
                address: miner.address(),
            }],
            1,
        );
        sign_transaction(&mut spend, &miner).unwrap();
        let mut block = chain.block_template(Address::ZERO, &[spend]).unwrap();
        block.header.nonce = 61;
        block.header.timestamp_ms = 61_000;
        sync_coinbase_timestamp(&mut block);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        assert!(matches!(
            chain.admit_block(block),
            Err(AdmitError::ImmatureCoinbase(_))
        ));

        // Mature after enough blue-score depth.
        let _ = mine_one(&mut chain, 62);
        let _ = mine_one(&mut chain, 63);
        let mut spend2 = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: coinbase_txid,
                    index: 0,
                },
            }],
            vec![TxOut {
                value,
                address: miner.address(),
            }],
            2,
        );
        sign_transaction(&mut spend2, &miner).unwrap();
        let mut ok = chain.block_template(Address::ZERO, &[spend2]).unwrap();
        ok.header.nonce = 64;
        ok.header.timestamp_ms = 64_000;
        sync_coinbase_timestamp(&mut ok);
        let epoch = chain.randomx_epoch_for_parents(&ok.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&ok.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= ok.header.bits);
        chain.admit_block(ok).unwrap();
    }

    #[test]
    fn virtual_tip_survives_bootstrap() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let tip = mine_one(&mut chain, 33);
        assert_eq!(chain.virtual_tip().unwrap(), tip);
        let reloaded = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(reloaded.virtual_tip().unwrap(), tip);
        assert_eq!(reloaded.genesis(), genesis);
    }

    fn mine_block_pow(chain: &ChainState, parents: &[Hash], nonce: u64) -> Block {
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.parents = parents.to_vec();
        block.header.bits = chain.expected_bits_for_parents(parents).unwrap();
        block.header.nonce = nonce;
        // Target-spaced timestamps keep the DAA at the difficulty floor (see mine_one).
        block.header.timestamp_ms = nonce.saturating_mul(1_000);
        let blue_score = chain
            .simulate_blue_score(parents, block.header.bits)
            .unwrap_or_else(|_| chain.estimate_blue_score(parents));
        let emission = chain.emission.reward_at_blue_score(blue_score);
        block.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: Address::ZERO,
            }],
            coinbase_commitment_nonce(
                &block.header.parents,
                block.header.timestamp_ms,
                nonce as u32,
            ),
        )];
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
        assert!(LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits);
        block
    }

    #[test]
    fn block_locator_and_headers_after_locator() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();

        let mut tip = genesis;
        for nonce in 1..=12u64 {
            tip = mine_child(&mut chain, &[tip], Address::ZERO, nonce);
        }

        let locator = chain.block_locator().unwrap();
        assert_eq!(locator.first().copied(), Some(tip));
        assert_eq!(locator.last().copied(), Some(genesis));
        assert!(locator.len() <= agora_p2p::MAX_LOCATOR_HASHES);

        // Peer at genesis asks for headers toward tip.
        let headers = chain.headers_after_locator(&[genesis], 5, None).unwrap();
        assert_eq!(headers.len(), 5);
        assert!(headers[0].parents.contains(&genesis));
        for window in headers.windows(2) {
            assert!(window[1].parents.contains(&window[0].hash()));
        }

        // Already at tip → empty.
        assert!(chain
            .headers_after_locator(&[tip], 100, None)
            .unwrap()
            .is_empty());
    }

    /// Lagging peer catches up via small GetHeaders batches + body admit (Phase 36).
    #[test]
    fn multiblock_headers_first_catchup() {
        use agora_p2p::validate_header_chain;
        use std::collections::BTreeSet;

        let store_a = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store_a).unwrap();
        let mut chain_a = ChainState::bootstrap(
            store_a,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();

        let mut tip = genesis;
        const DEPTH: u64 = 6;
        for nonce in 1..=DEPTH {
            tip = mine_child(&mut chain_a, &[tip], Address::ZERO, nonce);
        }
        assert_eq!(chain_a.virtual_tip().unwrap(), tip);

        let store_b = Arc::new(StateStore::open_in_memory());
        let genesis_b = GenesisBuilder::default().ignite(&store_b).unwrap();
        assert_eq!(genesis_b, genesis);
        let mut chain_b = ChainState::bootstrap(
            store_b,
            genesis_b,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(chain_b.virtual_tip().unwrap(), genesis);

        // Force several GetHeaders rounds (limit 2) to mimic multi-batch IBD.
        let mut rounds = 0u32;
        loop {
            rounds += 1;
            assert!(rounds < 20, "catch-up did not finish");
            let locator = chain_b.block_locator().unwrap();
            let headers = chain_a.headers_after_locator(&locator, 2, None).unwrap();
            if headers.is_empty() {
                break;
            }
            validate_header_chain(&headers).unwrap();
            for header in headers {
                let id = header.hash();
                let block = chain_a
                    .load_block(&id)
                    .unwrap()
                    .unwrap_or_else(|| panic!("missing body for {}", id.to_hex()));
                chain_b.admit_block(block).unwrap();
            }
        }
        assert!(
            rounds > 1,
            "expected multi-batch header sync, got {rounds} round(s)"
        );

        let tips_a: BTreeSet<_> = chain_a.tips().unwrap().into_iter().collect();
        let tips_b: BTreeSet<_> = chain_b.tips().unwrap().into_iter().collect();
        assert_eq!(tips_a, tips_b);
        assert_eq!(
            chain_a.virtual_tip().unwrap(),
            chain_b.virtual_tip().unwrap()
        );
        assert_eq!(chain_b.confirmations(&tip), Some(1));
        assert!(chain_b.confirmations(&genesis).unwrap() >= DEPTH);
    }

    #[test]
    fn orphan_pool_recovers_out_of_order_child() {
        use agora_p2p::{drain_orphans_after, OrphanPool};
        use std::time::Duration;

        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();

        let mid = mine_block_pow(&chain, &[genesis], 70);
        let mid_id = mid.id();
        let tip = mine_block_pow(&chain, &[mid_id], 71);
        let tip_id = tip.id();

        assert!(matches!(
            chain.admit_block(tip.clone()),
            Err(AdmitError::MissingParent(h)) if h == mid_id
        ));
        assert_eq!(chain.missing_parents_of(&tip), vec![mid_id]);

        let mut orphans = OrphanPool::new(Duration::from_secs(60), 16);
        assert!(orphans.park(tip, &[mid_id], None));

        let mid_admitted = chain.admit_block(mid).unwrap();
        assert_eq!(mid_admitted, mid_id);

        let drained = drain_orphans_after(&mut orphans, mid_id, |child| {
            match chain.admit_block(child.clone()) {
                Ok(id) => Ok(id),
                Err(AdmitError::MissingParent(_)) => {
                    let missing = chain.missing_parents_of(&child);
                    if missing.is_empty() {
                        Err(None)
                    } else {
                        Err(Some(missing))
                    }
                }
                Err(_) => Err(None),
            }
        });
        assert!(drained.contains(&tip_id));
        assert!(orphans.is_empty());
        assert!(chain.has_block(&tip_id).unwrap());
        assert_eq!(chain.virtual_tip().unwrap(), tip_id);
    }

    #[test]
    fn ghostdag_persist_matches_recompute_after_restart() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let a = mine_one(&mut chain, 11);
        let b = mine_one(&mut chain, 12);
        let live_a = chain.ghostdag.snapshot(&a).unwrap();
        let live_b = chain.ghostdag.snapshot(&b).unwrap();
        let tip = chain.virtual_tip().unwrap();
        let tip_work = chain.ghostdag.blue_work(&tip).unwrap();

        let reloaded = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(reloaded.ghostdag.snapshot(&a).unwrap(), live_a);
        assert_eq!(reloaded.ghostdag.snapshot(&b).unwrap(), live_b);
        assert_eq!(reloaded.virtual_tip().unwrap(), tip);
        assert_eq!(reloaded.ghostdag.blue_work(&tip).unwrap(), tip_work);
    }

    #[test]
    fn merge_of_conflicting_sibling_spends_is_accepted() {
        use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
        use agora_types::{OutPoint, TxIn};

        const PHRASE: &str =
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let store = Arc::new(StateStore::open_in_memory());
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let genesis = GenesisBuilder::default()
            .with_premine_address(alice.address())
            .ignite(&store)
            .unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let genesis_block = chain.load_block(&genesis).unwrap().unwrap();
        let premine_txid = genesis_block.transactions[0].tx_id();
        let premine = genesis_block.transactions[0].outputs[0].value;

        let mk_spend = |nonce: u64| {
            let mut tx = Transaction::unsigned(
                1,
                vec![TxIn {
                    previous_outpoint: OutPoint {
                        tx_id: premine_txid,
                        index: 0,
                    },
                }],
                vec![TxOut {
                    value: premine,
                    address: alice.address(),
                }],
                nonce,
            );
            sign_transaction(&mut tx, &alice).unwrap();
            tx
        };

        let mine_pow = |chain: &ChainState, block: &mut Block| {
            let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
            for nonce in 0..50_000u64 {
                block.header.nonce = nonce;
                let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
                if LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits {
                    break;
                }
            }
        };

        // Build both conflicting siblings while tip is still genesis (same UTXO view).
        let mut a_block = chain.block_template(Address::ZERO, &[mk_spend(1)]).unwrap();
        a_block.header.parents = vec![genesis];
        a_block.header.bits = chain.expected_bits_for_parents(&[genesis]).unwrap();
        a_block.header.timestamp_ms = 70_000;
        sync_coinbase_commitment_with(&mut a_block, 1);
        mine_pow(&chain, &mut a_block);

        let mut b_block = chain.block_template(Address::ZERO, &[mk_spend(2)]).unwrap();
        b_block.header.parents = vec![genesis];
        b_block.header.bits = chain.expected_bits_for_parents(&[genesis]).unwrap();
        b_block.header.timestamp_ms = 71_000;
        sync_coinbase_commitment_with(&mut b_block, 2);
        mine_pow(&chain, &mut b_block);

        let a = chain.admit_block(a_block).unwrap();
        // B is valid against genesis alone; blue_order(B) unapplies A then applies B.
        let b = chain.admit_block(b_block).unwrap();

        // Merge C parents both A and B — both may be blue; first spend in blue_order wins,
        // the conflicting sibling transfer is skipped (merge stays live).
        let mut c = chain.block_template(Address::ZERO, &[]).unwrap();
        c.header.parents = vec![a, b];
        c.header.bits = chain.expected_bits_for_parents(&[a, b]).unwrap();
        c.header.timestamp_ms = 72_000;
        let blue_score = chain.simulate_blue_score(&[a, b], c.header.bits).unwrap();
        let emission = chain.emission.reward_at_blue_score(blue_score);
        c.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: Address::ZERO,
            }],
            coinbase_commitment_nonce(&c.header.parents, c.header.timestamp_ms, 3),
        )];
        c.header.tx_root = Block::compute_tx_root(&c.transactions);
        let epoch = chain.randomx_epoch_for_parents(&c.header.parents);
        for nonce in 0..50_000u64 {
            c.header.nonce = nonce;
            let digest = RandomXPowHasher.pow_hash_with_epoch(&c.header, epoch);
            if LeadingZeroPow::leading_zero_bits(&digest) >= c.header.bits {
                break;
            }
        }
        let merged = chain.admit_block(c).unwrap();
        assert!(chain.has_block(&merged).unwrap());
        assert!(chain.dag.contains(&merged));
    }

    #[test]
    fn merge_of_duplicate_sibling_coinbases_is_accepted() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();

        // Force identical coinbases (same parents, timestamp, payout, extranonce) so
        // txids collide — Virtual apply must skip the duplicate rather than stall.
        let mut a_block = chain.block_template(Address::ZERO, &[]).unwrap();
        a_block.header.parents = vec![genesis];
        a_block.header.bits = chain.expected_bits_for_parents(&[genesis]).unwrap();
        a_block.header.timestamp_ms = 80_000;
        sync_coinbase_commitment_with(&mut a_block, 7);
        let twin_coinbase = a_block.transactions[0].clone();
        let epoch = chain.randomx_epoch_for_parents(&a_block.header.parents);
        for nonce in 0..50_000u64 {
            a_block.header.nonce = nonce;
            let digest = RandomXPowHasher.pow_hash_with_epoch(&a_block.header, epoch);
            if LeadingZeroPow::leading_zero_bits(&digest) >= a_block.header.bits {
                break;
            }
        }

        let mut b_block = a_block.clone();
        // Different PoW nonce → different block id, same coinbase body/txid.
        for nonce in 0..50_000u64 {
            b_block.header.nonce = nonce.wrapping_add(1_000);
            let digest = RandomXPowHasher.pow_hash_with_epoch(&b_block.header, epoch);
            if LeadingZeroPow::leading_zero_bits(&digest) >= b_block.header.bits
                && b_block.id() != a_block.id()
            {
                break;
            }
        }
        assert_eq!(b_block.transactions[0].tx_id(), twin_coinbase.tx_id());

        let a = chain.admit_block(a_block).unwrap();
        let b = chain.admit_block(b_block).unwrap();

        let mut c = chain.block_template(Address::ZERO, &[]).unwrap();
        c.header.parents = vec![a, b];
        c.header.bits = chain.expected_bits_for_parents(&[a, b]).unwrap();
        c.header.timestamp_ms = 81_000;
        let blue_score = chain.simulate_blue_score(&[a, b], c.header.bits).unwrap();
        let emission = chain.emission.reward_at_blue_score(blue_score);
        c.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: Address::ZERO,
            }],
            coinbase_commitment_nonce(&c.header.parents, c.header.timestamp_ms, 9),
        )];
        c.header.tx_root = Block::compute_tx_root(&c.transactions);
        let epoch = chain.randomx_epoch_for_parents(&c.header.parents);
        for nonce in 0..50_000u64 {
            c.header.nonce = nonce;
            let digest = RandomXPowHasher.pow_hash_with_epoch(&c.header, epoch);
            if LeadingZeroPow::leading_zero_bits(&digest) >= c.header.bits {
                break;
            }
        }
        let merged = chain.admit_block(c).unwrap();
        assert!(chain.has_block(&merged).unwrap());
    }

    #[test]
    fn parallel_siblings_share_expected_bits() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store,
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let bits_a = chain.expected_bits_for_parents(&[genesis]).unwrap();
        let a = mine_child(&mut chain, &[genesis], Address([0x11; 20]), 30);
        let bits_b = chain.expected_bits_for_parents(&[genesis]).unwrap();
        assert_eq!(
            bits_a, bits_b,
            "sibling from same parents must see identical expected bits"
        );
        let b = mine_child(&mut chain, &[genesis], Address([0x22; 20]), 31);
        assert_ne!(a, b);
        let tips = chain.tips().unwrap();
        assert!(tips.contains(&a) && tips.contains(&b));
    }

    #[test]
    fn invalid_signature_rejected_before_tips() {
        use agora_types::{OutPoint, TxIn};

        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let parent = mine_one(&mut chain, 40);
        let tips_before = chain.tips().unwrap();

        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.parents = vec![parent];
        block.header.bits = chain.expected_bits_for_parents(&[parent]).unwrap();
        block.header.timestamp_ms = 50_000;
        sync_coinbase_timestamp(&mut block);
        // Fabricate a transfer with garbage auth so overlay validation fails.
        let fake_tx = Transaction {
            version: 1,
            inputs: vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash([9u8; 32]),
                    index: 0,
                },
            }],
            outputs: vec![TxOut {
                value: Amount::from_base_units(1),
                address: Address::ZERO,
            }],
            nonce: 1,
            public_key: vec![1; 33],
            signature: vec![2; 64],
        };
        block.transactions.push(fake_tx);
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let epoch = chain.randomx_epoch_for_parents(&block.header.parents);
        // Mine a valid PoW so we reach UTXO/auth checks.
        for nonce in 0..10_000u64 {
            block.header.nonce = nonce;
            let digest = RandomXPowHasher.pow_hash_with_epoch(&block.header, epoch);
            if LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits {
                break;
            }
        }
        let rejected_id = block.id();
        let err = chain.admit_block(block).unwrap_err();
        assert!(
            matches!(err, AdmitError::Utxo(_)),
            "expected UTXO/auth rejection, got {err:?}"
        );
        assert_eq!(chain.tips().unwrap(), tips_before);
        assert!(!chain.dag.contains(&rejected_id));
        assert!(!chain.has_block(&rejected_id).unwrap());
    }

    #[test]
    fn pending_virtual_recovers_after_crash_marker() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        let tip = mine_one(&mut chain, 21);
        assert_eq!(chain.virtual_tip().unwrap(), tip);

        // Simulate a crash after marking pending but before tip meta advanced: tip meta
        // still points at genesis while UTXO already follows `tip` (journal present).
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::VIRTUAL_TIP,
                genesis.as_bytes(),
            )
            .unwrap();
        store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::PENDING_VIRTUAL,
                tip.as_bytes(),
            )
            .unwrap();

        let recovered = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            StoragePolicy::default(),
        )
        .unwrap();
        assert_eq!(recovered.virtual_tip().unwrap(), tip);
        assert!(store
            .get_cf(ColumnFamily::Meta, meta_keys::PENDING_VIRTUAL)
            .unwrap()
            .is_none());
        assert!(load_utxo_journal(store.as_ref(), &tip).unwrap().is_some());
    }
}
