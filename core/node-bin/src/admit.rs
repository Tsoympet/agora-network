//! Block admission: PoW → persist DAG → GHOSTDAG → reorg virtual UTXO.
//!
//! Live `cf_utxo` follows blues of `order_past(virtual_tip)` (selected tip by
//! blue_score). Non-selected tips are stored but do not spend until they join
//! the virtual blue set. Difficulty (`header.bits`) uses [`DaaConfig`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::{
    next_difficulty_weighted, work_from_bits, ConsensusLimits, DaaConfig, DaaSample, Dag,
    Difficulty, EmissionSchedule, Ghostdag, GhostdagConfig, LeadingZeroPow, PowAlgorithm,
    PowVerifier,
};
use agora_state_machine::{
    apply_block, delete_utxo_journal, index_block_transactions, load_header, load_utxo_journal,
    lookup_tx_location, meta_keys, revert_journal, store_header, store_utxo_journal,
    sum_transfer_fees, ColumnFamily, StateStore,
};
use agora_types::{Address, Amount, Block, BlockHeader, Hash, Transaction, TxOut};
use thiserror::Error;
use tracing::debug;

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
}

/// Runtime consensus knobs loaded from [`agora_state_machine::ChainParams`].
#[derive(Debug, Clone)]
pub struct ChainBootConfig {
    pub pow: PowAlgorithm,
    pub initial_bits: u32,
    pub daa: DaaConfig,
    pub ghostdag: GhostdagConfig,
    pub emission: EmissionSchedule,
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
        }
    }
}

impl From<&agora_state_machine::ChainParams> for ChainBootConfig {
    fn from(params: &agora_state_machine::ChainParams) -> Self {
        Self {
            pow: params.pow_algorithm,
            initial_bits: params.bits,
            daa: params.daa.clone(),
            ghostdag: params.ghostdag_config(),
            emission: params.emission.clone(),
        }
    }
}

impl ChainState {
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

        let chain = Self {
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
        };
        // Fresh / upgraded datadirs: ensure virtual tip meta exists.
        if chain.load_virtual_tip()?.is_none() {
            let tip = chain.select_virtual_tip()?.unwrap_or(genesis);
            chain.persist_virtual_tip(tip)?;
        }
        Ok(chain)
    }

    pub fn genesis(&self) -> Hash {
        self.genesis
    }

    pub fn virtual_tip(&self) -> Result<Hash, AdmitError> {
        Ok(self.load_virtual_tip()?.unwrap_or(self.genesis))
    }

    pub fn store(&self) -> &Arc<StateStore> {
        &self.store
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

    /// Confirmations for a block: `virtual_blue_score − block_blue_score + 1`.
    pub fn confirmations(&self, block_id: &Hash) -> Option<u64> {
        let block_score = self.ghostdag.blue_score(block_id)?;
        let tip = self.virtual_tip().ok()?;
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
        let parents = self.select_template_parents()?;
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let scheduled = self
            .emission
            .reward_at_blue_score(self.estimate_blue_score(&parents));
        let emission = self.clamp_emission(scheduled)?;
        let fees = sum_transfer_fees(self.store.as_ref(), transfers)
            .map_err(|e| AdmitError::Utxo(e.to_string()))?;
        let reward = emission
            .checked_add(fees)
            .ok_or_else(|| AdmitError::Utxo("coinbase reward overflow".into()))?;
        // Nonce = timestamp keeps coinbase txids unique across templates.
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(reward),
                address: payout,
            }],
            timestamp_ms,
        );
        let max_transfers = self.limits.max_block_transactions.saturating_sub(1);
        let mut transactions = Vec::with_capacity(1 + transfers.len().min(max_transfers));
        transactions.push(coinbase);
        transactions.extend(transfers.iter().take(max_transfers).cloned());
        let tx_root = Block::compute_tx_root(&transactions);
        Ok(Block {
            header: BlockHeader {
                version: 1,
                parents,
                timestamp_ms,
                bits: self.difficulty.as_bits(),
                nonce: 0,
                tx_root,
            },
            transactions,
        })
    }

    /// Tips for mining: highest blue_score first, capped at `max_block_parents`.
    fn select_template_parents(&self) -> Result<Vec<Hash>, AdmitError> {
        let mut tips = self.tips()?;
        tips.sort_by(|a, b| {
            let sa = self.ghostdag.blue_score(a).unwrap_or(0);
            let sb = self.ghostdag.blue_score(b).unwrap_or(0);
            sb.cmp(&sa).then_with(|| b.as_bytes().cmp(a.as_bytes()))
        });
        tips.truncate(self.limits.max_block_parents);
        Ok(tips)
    }

    /// Estimate blue score for coinbase budgeting before GHOSTDAG colors the block.
    fn estimate_blue_score(&self, parents: &[Hash]) -> u64 {
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

    /// Walk the selected-parent spine collecting work-weighted DAA samples (oldest → newest).
    fn daa_window(&self, tip: Hash) -> Result<Vec<DaaSample>, AdmitError> {
        let want = (self.daa.window_size as usize).saturating_add(1);
        let mut newest_first = Vec::with_capacity(want.min(64));
        let mut cursor = Some(tip);
        let mut cumulative = 0u128;
        while let Some(hash) = cursor {
            if newest_first.len() >= want {
                break;
            }
            if let Some(block) = self.load_block(&hash)? {
                let work = work_from_bits(block.header.bits);
                // Accumulate from tip backward, then reverse and rebuild cumulative oldest→newest.
                newest_first.push((block.header.timestamp_ms, work));
            }
            cursor = self.ghostdag.selected_parent(&hash);
        }
        newest_first.reverse();
        let mut samples = Vec::with_capacity(newest_first.len());
        for (timestamp_ms, work) in newest_first {
            cumulative = cumulative.saturating_add(work);
            samples.push(DaaSample {
                timestamp_ms,
                blue_work: cumulative,
            });
        }
        Ok(samples)
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

    /// Verify PoW + DAA bits, persist body, color with GHOSTDAG, reorg virtual UTXO.
    pub fn admit_block(&mut self, block: Block) -> Result<Hash, AdmitError> {
        let id = block.id();
        if self.dag.contains(&id) {
            return Err(AdmitError::Duplicate(id.to_hex()));
        }

        self.check_parents(&block)?;
        self.check_size_limits(&block)?;
        self.check_timestamps(&block)?;
        self.check_coinbase_maturity(&block)?;

        let expected = self.difficulty.as_bits();
        if block.header.bits != expected {
            return Err(AdmitError::WrongDifficulty {
                expected,
                got: block.header.bits,
            });
        }

        if block.header.tx_root != Block::compute_tx_root(&block.transactions) {
            return Err(AdmitError::BadTxRoot);
        }

        let pow_hash = self.pow.hasher().pow_hash(&block.header);
        self.pow
            .verify(&block.header, &pow_hash)
            .map_err(|_| AdmitError::InvalidPow)?;

        // Persist + color first — UTXO follows virtual tip after GHOSTDAG.
        self.persist_block(&block, id)?;
        self.dag
            .insert(id, block.header.parents.clone())
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        self.ghostdag
            .add_block(&self.dag, id)
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;

        let old_virtual = self.virtual_tip()?;
        let new_virtual = self
            .select_virtual_tip()?
            .ok_or_else(|| AdmitError::Consensus("no virtual tip".into()))?;

        // On failure, `reorg_utxo_to_virtual` restores UTXO to `old_virtual` blues.
        self.reorg_utxo_to_virtual(old_virtual, new_virtual)?;
        self.persist_virtual_tip(new_virtual)?;

        let window = self.daa_window(new_virtual)?;
        self.difficulty = next_difficulty_weighted(&self.daa, self.difficulty, &window);
        self.persist_difficulty()?;

        if let Err(err) = self.prune_hot_window() {
            debug!(error = %err, "hot prune skipped");
        }

        Ok(id)
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
        let mut parent_max = 0u64;
        for parent in &block.header.parents {
            if let Some(p) = self.load_block(parent)? {
                parent_max = parent_max.max(p.header.timestamp_ms);
            }
        }
        if block.header.timestamp_ms < parent_max {
            return Err(AdmitError::TimestampBeforeParent {
                ts: block.header.timestamp_ms,
                parent_ts: parent_max,
            });
        }
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
    fn check_coinbase_maturity(&self, block: &Block) -> Result<(), AdmitError> {
        let next_score = self.estimate_blue_score(&block.header.parents);
        for tx in &block.transactions {
            if tx.inputs.is_empty() {
                continue;
            }
            for input in &tx.inputs {
                let op = input.previous_outpoint;
                let Some((block_id, idx)) = lookup_tx_location(self.store.as_ref(), &op.tx_id)
                    .map_err(|e| AdmitError::Storage(e.to_string()))?
                else {
                    continue;
                };
                if block_id == self.genesis {
                    continue; // Premine spendable immediately.
                }
                let Some(created) = self.load_block(&block_id)? else {
                    continue;
                };
                let Some(created_tx) = created.transactions.get(idx as usize) else {
                    continue;
                };
                if !created_tx.inputs.is_empty() {
                    continue;
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

    fn persist_issued_supply(&self, issued: u64) -> Result<(), AdmitError> {
        self.store
            .put_cf(
                ColumnFamily::Meta,
                meta_keys::ISSUED_SUPPLY,
                &issued.to_le_bytes(),
            )
            .map_err(|e| AdmitError::Storage(e.to_string()))
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
        let tips = self.tips()?;
        Ok(self.ghostdag.select_virtual_tip(&tips))
    }

    /// Blues of `order_past(tip)` in apply order (genesis first).
    fn applied_blues(&self, tip: Hash) -> Result<Vec<Hash>, AdmitError> {
        self.ghostdag
            .blue_order(&self.dag, tip)
            .map_err(|e| AdmitError::Consensus(e.to_string()))
    }

    /// Sync live UTXO from blues(`old`) → blues(`new`) via durable journals.
    fn reorg_utxo_to_virtual(&mut self, old: Hash, new: Hash) -> Result<(), AdmitError> {
        let applied = self.applied_blues(old)?;
        let target = self.applied_blues(new)?;
        let prefix = common_prefix_len(&applied, &target);

        for hash in applied[prefix..].iter().rev() {
            self.unapply_block_from_virtual(*hash)?;
        }

        for (offset, hash) in target[prefix..].iter().enumerate() {
            if let Err(err) = self.apply_block_to_virtual(*hash) {
                for undo in target[prefix..prefix + offset].iter().rev() {
                    let _ = self.unapply_block_from_virtual(*undo);
                }
                for redo in &applied[prefix..] {
                    let _ = self.apply_block_to_virtual(*redo);
                }
                return Err(err);
            }
        }
        Ok(())
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
        let fees = sum_transfer_fees(self.store.as_ref(), &block.transactions)
            .map_err(|e| AdmitError::Utxo(e.to_string()))?;
        let coinbase_total = Self::coinbase_output_sum(&block)?;
        let subsidy = coinbase_total.saturating_sub(fees);
        if subsidy > emission {
            return Err(AdmitError::Utxo(format!(
                "coinbase subsidy {subsidy} exceeds clamped emission {emission}"
            )));
        }
        let max = self.load_max_supply()?;
        let issued = self.load_issued_supply()?;
        if issued.saturating_add(subsidy) > max {
            return Err(AdmitError::SupplyCapExceeded);
        }
        let journal = apply_block(self.store.as_ref(), &block, emission)
            .map_err(|e| AdmitError::Utxo(e.to_string()))?;
        store_utxo_journal(self.store.as_ref(), &hash, &journal)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.persist_issued_supply(issued.saturating_add(subsidy))?;
        Ok(())
    }

    fn unapply_block_from_virtual(&self, hash: Hash) -> Result<(), AdmitError> {
        if hash == self.genesis {
            return Ok(());
        }
        let Some(journal) = load_utxo_journal(self.store.as_ref(), &hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        else {
            return Err(AdmitError::Utxo(format!(
                "missing utxo journal for {}",
                hash.to_hex()
            )));
        };
        let block = self
            .load_block(&hash)?
            .ok_or_else(|| AdmitError::Storage(format!("missing block {}", hash.to_hex())))?;
        // Fees = coinbase − subsidy; recover subsidy from journal created coinbase outs vs spent.
        // Prefer: coinbase_total − Σ(spent values − created non-coinbase) is messy.
        // Recompute fees from journal spent/created is hard; use coinbase_total − (sum spent - sum
        // non-coinbase created)... Simpler: fees were `coinbase_total - subsidy` and subsidy was
        // tracked only in issued_supply. Recover via coinbase_total and fee from spent inputs:
        let coinbase_total = Self::coinbase_output_sum(&block)?;
        let mut input_sum = 0u64;
        let mut transfer_out = 0u64;
        for tx in &block.transactions {
            if tx.inputs.is_empty() {
                continue;
            }
            for input in &tx.inputs {
                if let Some((_, out)) = journal
                    .spent
                    .iter()
                    .find(|(op, _)| op == &input.previous_outpoint)
                {
                    input_sum = input_sum.saturating_add(out.value.as_base_units());
                }
            }
            for out in &tx.outputs {
                transfer_out = transfer_out.saturating_add(out.value.as_base_units());
            }
        }
        let fees = input_sum.saturating_sub(transfer_out);
        let subsidy = coinbase_total.saturating_sub(fees);

        revert_journal(self.store.as_ref(), &journal)
            .map_err(|e| AdmitError::Utxo(e.to_string()))?;
        delete_utxo_journal(self.store.as_ref(), &hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        let issued = self.load_issued_supply()?;
        self.persist_issued_supply(issued.saturating_sub(subsidy))?;
        Ok(())
    }

    fn coinbase_output_sum(block: &Block) -> Result<u64, AdmitError> {
        let coinbase = block
            .transactions
            .iter()
            .find(|tx| tx.inputs.is_empty())
            .ok_or_else(|| AdmitError::Utxo("missing coinbase".into()))?;
        let mut total = 0u64;
        for out in &coinbase.outputs {
            total = total
                .checked_add(out.value.as_base_units())
                .ok_or_else(|| AdmitError::Utxo("coinbase overflow".into()))?;
        }
        Ok(total)
    }

    fn persist_block(&self, block: &Block, id: Hash) -> Result<(), AdmitError> {
        let block_bytes = borsh::to_vec(block).map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Hot, id.as_bytes(), &block_bytes)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        if self.storage.archival {
            self.store
                .put_cf(ColumnFamily::Archival, id.as_bytes(), &block_bytes)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
        }
        store_header(self.store.as_ref(), &id, &block.header)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        index_block_transactions(self.store.as_ref(), block)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;

        let mut tips = self.tips()?;
        tips.retain(|t| !block.header.parents.iter().any(|p| p == t));
        if !tips.contains(&id) {
            tips.push(id);
        }
        let tips_bytes = borsh::to_vec(&tips).map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Meta, meta_keys::TIPS, &tips_bytes)
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

    let mut dag = Dag::new();
    dag.insert(genesis, vec![])
        .map_err(|e| AdmitError::Consensus(e.to_string()))?;
    let mut ghostdag = Ghostdag::new(ghostdag_config);
    ghostdag
        .add_block(&dag, genesis)
        .map_err(|e| AdmitError::Consensus(e.to_string()))?;

    while !pending.is_empty() {
        let ready: Vec<Hash> = pending
            .iter()
            .filter(|(_, parents)| parents.iter().all(|p| dag.contains(p)))
            .map(|(hash, _)| *hash)
            .collect();
        if ready.is_empty() {
            return Err(AdmitError::Storage(
                "cannot rebuild dag: missing parents or cycle".into(),
            ));
        }
        for hash in ready {
            let parents = pending.remove(&hash).expect("ready hash");
            dag.insert(hash, parents)
                .map_err(|e| AdmitError::Consensus(e.to_string()))?;
            ghostdag
                .add_block(&dag, hash)
                .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        }
    }

    Ok((dag, ghostdag))
}

fn common_prefix_len(a: &[Hash], b: &[Hash]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
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
    use agora_consensus::{PowHasher, RandomXPowHasher};
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
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
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
        assert_eq!(reloaded.difficulty().as_bits(), next.level);
        assert_eq!(
            reloaded
                .block_template(Address::ZERO, &[])
                .unwrap()
                .header
                .bits,
            next.level
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
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
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

    fn mine_one(chain: &mut ChainState, nonce: u64) -> Hash {
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.nonce = nonce;
        block.header.timestamp_ms = nonce;
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
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
        let mut chain = ChainState::bootstrap(
            store.clone(),
            genesis,
            PowAlgorithm::RandomX,
            0,
            policy.clone(),
        )
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
        block.header.nonce = nonce;
        block.header.timestamp_ms = nonce;
        // Recompute coinbase for emission estimate from these parents.
        let emission = chain
            .emission
            .reward_at_blue_score(chain.estimate_blue_score(parents));
        block.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: payout,
            }],
            nonce,
        )];
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
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
        let emission = chain
            .emission
            .reward_at_blue_score(chain.estimate_blue_score(&[a, b]));
        block.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: Address::ZERO,
            }],
            42,
        )];
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
        assert!(matches!(
            chain.admit_block(block),
            Err(AdmitError::TooManyParents { .. })
        ));

        chain.limits.max_block_parents = 16;
        let mut early = chain.block_template(Address::ZERO, &[]).unwrap();
        early.header.timestamp_ms = 0;
        early.header.nonce = 43;
        early.header.tx_root = Block::compute_tx_root(&early.transactions);
        let digest = RandomXPowHasher.pow_hash(&early.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&early.header, &digest)
            .unwrap();
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
        block.header.timestamp_ms = 61;
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
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
        ok.header.timestamp_ms = 64;
        ok.header.tx_root = Block::compute_tx_root(&ok.transactions);
        let digest = RandomXPowHasher.pow_hash(&ok.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&ok.header, &digest)
            .unwrap();
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
        block.header.nonce = nonce;
        block.header.timestamp_ms = nonce;
        let emission = chain
            .emission
            .reward_at_blue_score(chain.estimate_blue_score(parents));
        block.transactions = vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission),
                address: Address::ZERO,
            }],
            nonce,
        )];
        block.header.tx_root = Block::compute_tx_root(&block.transactions);
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
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
}
