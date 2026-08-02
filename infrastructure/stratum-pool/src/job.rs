use agora_consensus::{KHeavyHashPowHasher, LeadingZeroPow, PowHasher};
use agora_types::{Block, BlockHeader, Hash};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Work template handed to ASIC miners (kHeavyHash path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningJob {
    pub job_id: String,
    /// Full template block (coinbase + transfers); PoW mines `header.nonce`.
    pub block: Block,
    /// Required leading zero bits on the kHeavyHash digest (network `header.bits`).
    pub difficulty_bits: u32,
}

impl MiningJob {
    pub fn new(job_id: impl Into<String>, block: Block, difficulty_bits: u32) -> Self {
        Self {
            job_id: job_id.into(),
            block,
            difficulty_bits,
        }
    }

    pub fn header(&self) -> &BlockHeader {
        &self.block.header
    }

    pub fn with_nonce(&self, nonce: u64) -> Block {
        let mut block = self.block.clone();
        block.header.nonce = nonce;
        block
    }

    pub fn pow_hash(&self, nonce: u64) -> Hash {
        KHeavyHashPowHasher.pow_hash(&self.with_nonce(nonce).header)
    }

    pub fn meets_target(&self, hash: &Hash) -> bool {
        leading_zero_bits(hash) >= self.difficulty_bits
    }
}

pub fn leading_zero_bits(hash: &Hash) -> u32 {
    LeadingZeroPow::leading_zero_bits(hash)
}

/// Compact share identity to reject duplicates.
pub fn share_id(job_id: &str, nonce: u64, worker: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(job_id.as_bytes());
    hasher.update(nonce.to_le_bytes());
    hasher.update(worker.as_bytes());
    hex::encode(hasher.finalize())
}
