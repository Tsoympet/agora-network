//! Block admission: PoW verify → UTXO apply → DAG/GHOSTDAG → durable store.
//!
//! Difficulty (`header.bits`) is driven by [`DaaConfig`] / [`next_difficulty`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::{
    next_difficulty_weighted, work_from_bits, DaaConfig, DaaSample, Dag, Difficulty,
    EmissionSchedule, Ghostdag, GhostdagConfig, LeadingZeroPow, PowAlgorithm, PowVerifier,
};
use agora_state_machine::{
    apply_block, index_block_transactions, meta_keys, revert_journal, sum_transfer_fees,
    ColumnFamily, StateStore,
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
    #[error("missing parent: {0}")]
    MissingParent(String),
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
}

/// Shared chain state mutated by RPC submit and gossip admission.
pub struct ChainState {
    store: Arc<StateStore>,
    dag: Dag,
    ghostdag: Ghostdag,
    pow: LeadingZeroPow,
    emission: EmissionSchedule,
    daa: DaaConfig,
    difficulty: Difficulty,
    storage: StoragePolicy,
}

impl ChainState {
    pub fn bootstrap(
        store: Arc<StateStore>,
        genesis: Hash,
        algo: PowAlgorithm,
        initial_bits: u32,
        storage: StoragePolicy,
    ) -> Result<Self, AdmitError> {
        let (dag, ghostdag) = rebuild_dag_from_store(store.as_ref(), genesis)?;

        let daa = DaaConfig {
            // Allow bits=0 testnets when operators start at zero.
            min_level: if initial_bits == 0 { 0 } else { 1 },
            ..DaaConfig::default()
        };
        let difficulty = load_or_init_difficulty(&store, initial_bits)?;

        Ok(Self {
            store,
            dag,
            ghostdag,
            pow: LeadingZeroPow::new(algo),
            emission: EmissionSchedule::default(),
            daa,
            difficulty,
            storage,
        })
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

    pub fn load_block(&self, hash: &Hash) -> Result<Option<Block>, AdmitError> {
        for cf in [ColumnFamily::Hot, ColumnFamily::Warm, ColumnFamily::Archival] {
            if let Some(bytes) = self
                .store
                .get_cf(cf, hash.as_bytes())
                .map_err(|e| AdmitError::Storage(e.to_string()))?
            {
                let block = borsh::from_slice(&bytes)
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
                return Ok(Some(block));
            }
        }
        Ok(None)
    }

    pub fn has_block(&self, hash: &Hash) -> Result<bool, AdmitError> {
        Ok(self.load_block(hash)?.is_some() || self.dag.contains(hash))
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
        let parents = self.tips()?;
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let emission = self
            .emission
            .reward_at_blue_score(self.estimate_blue_score(&parents));
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
        let mut transactions = Vec::with_capacity(1 + transfers.len());
        transactions.push(coinbase);
        transactions.extend(transfers.iter().cloned());
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

    /// Verify PoW + DAA bits, apply UTXOs, persist, update GHOSTDAG, retarget difficulty.
    pub fn admit_block(&mut self, block: Block) -> Result<Hash, AdmitError> {
        let id = block.id();
        if self.dag.contains(&id) {
            return Err(AdmitError::Duplicate(id.to_hex()));
        }
        for parent in &block.header.parents {
            if !self.dag.contains(parent) {
                return Err(AdmitError::MissingParent(parent.to_hex()));
            }
        }

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

        // Emission only — `apply_block` adds transfer fees into the coinbase budget.
        let emission = self
            .emission
            .reward_at_blue_score(self.estimate_blue_score(&block.header.parents));
        let journal = apply_block(self.store.as_ref(), &block, emission)
            .map_err(|e| AdmitError::Utxo(e.to_string()))?;

        if let Err(err) = self.persist_block(&block, id) {
            let _ = revert_journal(self.store.as_ref(), &journal);
            return Err(err);
        }

        self.dag
            .insert(id, block.header.parents.clone())
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        self.ghostdag
            .add_block(&self.dag, id)
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;

        let window = self.daa_window(id)?;
        self.difficulty = next_difficulty_weighted(&self.daa, self.difficulty, &window);
        self.persist_difficulty()?;

        if let Err(err) = self.prune_hot_window() {
            debug!(error = %err, "hot prune skipped");
        }

        Ok(id)
    }

    fn persist_block(&self, block: &Block, id: Hash) -> Result<(), AdmitError> {
        let block_bytes =
            borsh::to_vec(block).map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Hot, id.as_bytes(), &block_bytes)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        if self.storage.archival {
            self.store
                .put_cf(ColumnFamily::Archival, id.as_bytes(), &block_bytes)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
        }
        index_block_transactions(self.store.as_ref(), block)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;

        let mut tips = self.tips()?;
        tips.retain(|t| !block.header.parents.iter().any(|p| p == t));
        if !tips.contains(&id) {
            tips.push(id);
        }
        let tips_bytes =
            borsh::to_vec(&tips).map_err(|e| AdmitError::Storage(e.to_string()))?;
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
            if let Some(block) = self.load_block(&hash)? {
                for parent in block.header.parents {
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
    for cf in [ColumnFamily::Hot, ColumnFamily::Warm, ColumnFamily::Archival] {
        if let Some(bytes) = store
            .get_cf(cf, hash.as_bytes())
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        {
            let block = borsh::from_slice(&bytes)
                .map_err(|e| AdmitError::Storage(e.to_string()))?;
            return Ok(Some(block));
        }
    }
    Ok(None)
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
fn rebuild_dag_from_store(store: &StateStore, genesis: Hash) -> Result<(Dag, Ghostdag), AdmitError> {
    let mut tips = load_tips_meta(store)?;
    if tips.is_empty() {
        tips.push(genesis);
    }

    let mut pending: HashMap<Hash, Block> = HashMap::new();
    let mut stack = tips;
    while let Some(hash) = stack.pop() {
        if hash == genesis || pending.contains_key(&hash) {
            continue;
        }
        let block = load_block_bytes(store, &hash)?.ok_or_else(|| {
            AdmitError::Storage(format!("missing block {} while rebuilding dag", hash.to_hex()))
        })?;
        for parent in &block.header.parents {
            stack.push(*parent);
        }
        pending.insert(hash, block);
    }

    let mut dag = Dag::new();
    dag.insert(genesis, vec![])
        .map_err(|e| AdmitError::Consensus(e.to_string()))?;
    let mut ghostdag = Ghostdag::new(GhostdagConfig::default());
    ghostdag
        .add_block(&dag, genesis)
        .map_err(|e| AdmitError::Consensus(e.to_string()))?;

    while !pending.is_empty() {
        let ready: Vec<Hash> = pending
            .iter()
            .filter(|(_, block)| block.header.parents.iter().all(|p| dag.contains(p)))
            .map(|(hash, _)| *hash)
            .collect();
        if ready.is_empty() {
            return Err(AdmitError::Storage(
                "cannot rebuild dag: missing parents or cycle".into(),
            ));
        }
        for hash in ready {
            let block = pending.remove(&hash).expect("ready hash");
            dag.insert(hash, block.header.parents.clone())
                .map_err(|e| AdmitError::Consensus(e.to_string()))?;
            ghostdag
                .add_block(&dag, hash)
                .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        }
    }

    Ok((dag, ghostdag))
}

fn load_or_init_difficulty(
    store: &StateStore,
    initial_bits: u32,
) -> Result<Difficulty, AdmitError> {
    if let Some(bytes) = store
        .get_cf(ColumnFamily::Meta, meta_keys::DAA_DIFFICULTY)
        .map_err(|e| AdmitError::Storage(e.to_string()))?
    {
        if bytes.len() == 4 {
            let level = u32::from_le_bytes(bytes.try_into().unwrap());
            return Ok(Difficulty::new(level));
        }
    }
    let difficulty = Difficulty::new(initial_bits);
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
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0, StoragePolicy::default()).unwrap();
        assert_eq!(chain.difficulty().as_bits(), 0);
        assert_eq!(
            chain.block_template(Address::ZERO, &[]).unwrap().header.bits,
            0
        );

        let mut bad = chain.block_template(Address::ZERO, &[]).unwrap();
        bad.header.bits = 3;
        bad.header.nonce = 1;
        assert!(matches!(
            chain.admit_block(bad),
            Err(AdmitError::WrongDifficulty { expected: 0, got: 3 })
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

        let reloaded =
            ChainState::bootstrap(store, genesis, PowAlgorithm::RandomX, 0, StoragePolicy::default()).unwrap();
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
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0, StoragePolicy::default()).unwrap();
        let mut block = chain.block_template(Address::ZERO, &[]).unwrap();
        block.header.nonce = 3;
        let digest = RandomXPowHasher.pow_hash(&block.header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&block.header, &digest)
            .unwrap();
        let id = chain.admit_block(block).unwrap();

        let reloaded =
            ChainState::bootstrap(store, genesis, PowAlgorithm::RandomX, 0, StoragePolicy::default()).unwrap();
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
        // Tips still load.
        assert!(chain.load_block(&_b2).unwrap().is_some());
    }
}
