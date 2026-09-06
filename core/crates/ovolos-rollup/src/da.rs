//! Historical lab data commitments for Ovolos batches.
//!
//! This source type is not an Agora L1 transaction. In particular,
//! [`BatchCommitment::to_da_bytes`] must not be placed in a transfer memo.
//! `agora-layers-runtime` can map it into the provenance-bound Trident
//! commitment candidate that still requires a reviewed consensus lane.

use agora_types::Hash;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::{Batch, EvmTx};

/// Compact source commitment retained by the historical lab runtime.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct BatchCommitment {
    pub batch_id: Hash,
    pub sequence: u64,
    pub prev_state_root: Hash,
    pub post_state_root: Hash,
    pub tx_merkle_root: Hash,
    pub tx_count: u32,
    pub posted_at_ms: u64,
}

impl BatchCommitment {
    pub fn from_batch(batch: &Batch) -> Self {
        Self {
            batch_id: batch.id(),
            sequence: batch.sequence,
            prev_state_root: batch.prev_state_root,
            post_state_root: batch.post_state_root,
            tx_merkle_root: tx_merkle_root(&batch.transactions),
            tx_count: batch.transactions.len() as u32,
            posted_at_ms: batch.posted_at_ms,
        }
    }

    pub fn id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    /// Legacy source bytes for lab persistence and reproducibility only.
    ///
    /// These bytes have no L1 type/version/network binding and are therefore
    /// not suitable for direct RPC submission.
    pub fn to_da_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("borsh BatchCommitment")
    }

    pub fn from_da_bytes(bytes: &[u8]) -> Result<Self, String> {
        borsh::from_slice(bytes).map_err(|e| e.to_string())
    }
}

/// Binary merkle root over EVM tx digests (SHA-256 leaves, pairwise concat).
pub fn tx_merkle_root(txs: &[EvmTx]) -> Hash {
    if txs.is_empty() {
        return Hash::ZERO;
    }
    let mut level: Vec<[u8; 32]> = txs
        .iter()
        .map(|tx| {
            let mut hasher = Sha256::new();
            hasher.update(&tx.0);
            let d = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&d);
            out
        })
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for chunk in level.chunks(2) {
            let mut hasher = Sha256::new();
            hasher.update(chunk[0]);
            if chunk.len() == 2 {
                hasher.update(chunk[1]);
            } else {
                hasher.update(chunk[0]);
            }
            let d = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&d);
            next.push(out);
        }
        level = next;
    }
    Hash(level[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvmTx;

    #[test]
    fn commitment_roundtrip_bytes() {
        let batch = Batch {
            sequence: 1,
            prev_state_root: Hash::ZERO,
            post_state_root: Hash([1u8; 32]),
            transactions: vec![EvmTx(vec![1, 2, 3])],
            posted_at_ms: 42,
        };
        let c = BatchCommitment::from_batch(&batch);
        let bytes = c.to_da_bytes();
        let back = BatchCommitment::from_da_bytes(&bytes).unwrap();
        assert_eq!(c, back);
        assert_eq!(c.batch_id, batch.id());
    }
}
