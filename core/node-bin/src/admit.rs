//! Block admission: PoW verify → UTXO apply → DAG/GHOSTDAG → durable store.
//!
//! Difficulty (`header.bits`) is driven by [`DaaConfig`] / [`next_difficulty`].

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::{
    next_difficulty, DaaConfig, Dag, Difficulty, EmissionSchedule, Ghostdag, GhostdagConfig,
    LeadingZeroPow, PowAlgorithm, PowVerifier,
};
use agora_state_machine::{apply_block, meta_keys, revert_journal, ColumnFamily, StateStore};
use agora_types::{Block, BlockHeader, Hash};
use thiserror::Error;

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
}

impl ChainState {
    pub fn bootstrap(
        store: Arc<StateStore>,
        genesis: Hash,
        algo: PowAlgorithm,
        initial_bits: u32,
    ) -> Result<Self, AdmitError> {
        let mut dag = Dag::new();
        dag.insert(genesis, vec![])
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        let mut ghostdag = Ghostdag::new(GhostdagConfig::default());
        ghostdag
            .add_block(&dag, genesis)
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;

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

    /// Build a mining template parented to current tips with DAA `bits`.
    pub fn block_template(&self) -> Result<BlockHeader, AdmitError> {
        let parents = self.tips()?;
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(BlockHeader {
            version: 1,
            parents,
            timestamp_ms,
            bits: self.difficulty.as_bits(),
            nonce: 0,
            tx_root: Hash::ZERO,
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

    /// Walk the selected-parent spine collecting timestamps oldest → newest.
    fn timestamp_window(&self, tip: Hash) -> Result<Vec<u64>, AdmitError> {
        let want = (self.daa.window_size as usize).saturating_add(1);
        let mut newest_first = Vec::with_capacity(want.min(64));
        let mut cursor = Some(tip);
        while let Some(hash) = cursor {
            if newest_first.len() >= want {
                break;
            }
            if let Some(block) = self.load_block(&hash)? {
                newest_first.push(block.header.timestamp_ms);
            }
            cursor = self.ghostdag.selected_parent(&hash);
        }
        newest_first.reverse();
        Ok(newest_first)
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

        let pow_hash = self.pow.hasher().pow_hash(&block.header);
        self.pow
            .verify(&block.header, &pow_hash)
            .map_err(|_| AdmitError::InvalidPow)?;

        let reward = self
            .emission
            .reward_at_blue_score(self.estimate_blue_score(&block.header.parents));
        let journal = apply_block(self.store.as_ref(), &block, reward)
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

        let window = self.timestamp_window(id)?;
        self.difficulty = next_difficulty(&self.daa, self.difficulty, &window);
        self.persist_difficulty()?;

        Ok(id)
    }

    fn persist_block(&self, block: &Block, id: Hash) -> Result<(), AdmitError> {
        let block_bytes =
            borsh::to_vec(block).map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Hot, id.as_bytes(), &block_bytes)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .put_cf(ColumnFamily::Archival, id.as_bytes(), &block_bytes)
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
        let store = Arc::new(StateStore::open("/tmp/agora-daa-admit").unwrap());
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let mut chain =
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0).unwrap();
        assert_eq!(chain.difficulty().as_bits(), 0);
        assert_eq!(chain.block_template().unwrap().bits, 0);

        let mut header = chain.block_template().unwrap();
        header.bits = 3;
        header.nonce = 1;
        assert!(matches!(
            chain.admit_block(Block {
                header,
                transactions: vec![],
            }),
            Err(AdmitError::WrongDifficulty { expected: 0, got: 3 })
        ));

        // Admit a valid bits=0 block, then force a retarget from the spine window.
        let mut header = chain.block_template().unwrap();
        header.nonce = 9;
        header.timestamp_ms = 1;
        let digest = RandomXPowHasher.pow_hash(&header);
        LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&header, &digest)
            .unwrap();
        let id = chain
            .admit_block(Block {
                header,
                transactions: vec![],
            })
            .unwrap();

        chain.daa.window_size = 2;
        chain.daa.target_block_time_ms = 10_000;
        chain.difficulty = Difficulty::new(4);
        let window = chain.timestamp_window(id).unwrap();
        assert!(window.len() >= 2);
        let next = next_difficulty(&chain.daa, chain.difficulty, &window);
        assert!(next.level > 4);
        chain.difficulty = next;
        chain.persist_difficulty().unwrap();

        let reloaded =
            ChainState::bootstrap(store, genesis, PowAlgorithm::RandomX, 0).unwrap();
        assert_eq!(reloaded.difficulty().as_bits(), next.level);
        assert_eq!(reloaded.block_template().unwrap().bits, next.level);
    }
}
