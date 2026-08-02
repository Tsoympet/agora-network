//! Block admission: PoW verify → UTXO apply → DAG/GHOSTDAG → durable store.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agora_consensus::{
    Dag, EmissionSchedule, Ghostdag, GhostdagConfig, LeadingZeroPow, PowAlgorithm, PowVerifier,
};
use agora_state_machine::{apply_block, meta_keys, revert_journal, ColumnFamily, StateStore};
use agora_types::{Block, BlockHeader, Hash};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdmitError {
    #[error("invalid proof of work")]
    InvalidPow,
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
    /// Difficulty bits advertised in block templates.
    template_bits: u32,
}

impl ChainState {
    pub fn bootstrap(
        store: Arc<StateStore>,
        genesis: Hash,
        algo: PowAlgorithm,
        template_bits: u32,
    ) -> Result<Self, AdmitError> {
        let mut dag = Dag::new();
        dag.insert(genesis, vec![])
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        let mut ghostdag = Ghostdag::new(GhostdagConfig::default());
        ghostdag
            .add_block(&dag, genesis)
            .map_err(|e| AdmitError::Consensus(e.to_string()))?;
        Ok(Self {
            store,
            dag,
            ghostdag,
            pow: LeadingZeroPow::new(algo),
            emission: EmissionSchedule::default(),
            template_bits,
        })
    }

    pub fn store(&self) -> &Arc<StateStore> {
        &self.store
    }

    pub fn pow_algorithm(&self) -> PowAlgorithm {
        self.pow.algorithm()
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

    /// Build a mining template parented to current tips.
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
            bits: self.template_bits,
            nonce: 0,
            tx_root: Hash::ZERO,
        })
    }

    /// Estimate blue score for coinbase budgeting before GHOSTDAG colors the block.
    ///
    /// Uses `max(parent.blue_score) + 1`, which matches chain growth and is a safe
    /// upper bound for merge blocks in this scaffold.
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

    /// Verify PoW, apply UTXOs, persist, then update GHOSTDAG.
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
