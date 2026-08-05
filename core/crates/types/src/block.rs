use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hash, Transaction};

/// Block header for Agora's BlockDAG tips.
///
/// Multiple parents enable parallel block production; GHOSTDAG later imposes order.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct BlockHeader {
    pub version: u16,
    pub parents: Vec<Hash>,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub nonce: u64,
    pub tx_root: Hash,
}

impl BlockHeader {
    pub fn hash(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Full block: header + transactions.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn id(&self) -> Hash {
        self.header.hash()
    }

    /// Verify that `header.tx_root` commits to `transactions`.
    pub fn verify_tx_root(&self) -> bool {
        self.header.tx_root == Self::compute_tx_root(&self.transactions)
    }

    /// Domain-separated pairwise merkle root over transaction ids.
    ///
    /// - Leaf nodes tagged `0x00`, internal nodes tagged `0x01`, odd-level pad tagged `0x02`
    ///   (never duplicates the last leaf — avoids length-extension collisions).
    /// - Final digest mixes in the leaf count so `[A,B,C]` cannot equal `[A,B,C,C]`.
    pub fn compute_tx_root(transactions: &[Transaction]) -> Hash {
        let count = transactions.len() as u64;
        if transactions.is_empty() {
            let mut buf = [0u8; 9];
            buf[0] = 0x03; // empty-root tag
            buf[1..].copy_from_slice(&count.to_le_bytes());
            return Hash::hash_bytes(&buf);
        }

        let mut level: Vec<Hash> = transactions
            .iter()
            .map(|tx| {
                let mut buf = [0u8; 33];
                buf[0] = 0x00; // leaf
                buf[1..].copy_from_slice(tx.tx_id().as_bytes());
                Hash::hash_bytes(&buf)
            })
            .collect();

        while level.len() > 1 {
            if level.len() % 2 == 1 {
                // Domain-separated pad — not a duplicate of the last leaf.
                level.push(Hash::hash_bytes(&[0x02]));
            }
            level = level
                .chunks(2)
                .map(|pair| {
                    let mut buf = [0u8; 65];
                    buf[0] = 0x01; // internal
                    buf[1..33].copy_from_slice(pair[0].as_bytes());
                    buf[33..].copy_from_slice(pair[1].as_bytes());
                    Hash::hash_bytes(&buf)
                })
                .collect();
        }

        let mut final_buf = [0u8; 41];
        final_buf[0] = 0x04; // root-with-count tag
        final_buf[1..33].copy_from_slice(level[0].as_bytes());
        final_buf[33..].copy_from_slice(&count.to_le_bytes());
        Hash::hash_bytes(&final_buf)
    }
}
