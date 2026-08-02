//! Compact-block reconstruction and IBD fetch bookkeeping.
//!
//! Peers gossip [`crate::NetworkMessage::CompactBlock`] / `BlockAnnounce`, then
//! request missing bodies with `GetBlock`. Empty short-id lists reconstruct
//! immediately (common for coinbase-only / empty templates).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use agora_types::{Block, BlockHeader, Hash, Transaction};

/// First 8 bytes of a transaction id — BIP152-style short id (scaffold).
pub fn tx_short_id(tx_id: &Hash) -> [u8; 8] {
    let mut out = [0u8; 8];
    out.copy_from_slice(&tx_id.as_bytes()[..8]);
    out
}

/// Build short ids for a full block.
pub fn short_ids_for_block(block: &Block) -> Vec<[u8; 8]> {
    block
        .transactions
        .iter()
        .map(|tx| tx_short_id(&tx.tx_id()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconstructError {
    /// One or more short ids were not found in the local mempool.
    MissingShortIds(usize),
    /// Reassembled txs do not match `header.tx_root`.
    TxRootMismatch,
}

/// Rebuild a full block from a compact header + short ids via mempool lookup.
pub fn reconstruct_compact_block(
    header: BlockHeader,
    short_ids: &[[u8; 8]],
    mut lookup: impl FnMut(&[u8; 8]) -> Option<Transaction>,
) -> Result<Block, ReconstructError> {
    let mut transactions = Vec::with_capacity(short_ids.len());
    let mut missing = 0usize;
    for sid in short_ids {
        match lookup(sid) {
            Some(tx) => transactions.push(tx),
            None => missing += 1,
        }
    }
    if missing > 0 {
        return Err(ReconstructError::MissingShortIds(missing));
    }
    let root = Block::compute_tx_root(&transactions);
    if root != header.tx_root {
        return Err(ReconstructError::TxRootMismatch);
    }
    Ok(Block {
        header,
        transactions,
    })
}

/// Dedupes in-flight `GetBlock` requests with a simple TTL.
#[derive(Debug, Default)]
pub struct PendingFetches {
    pending: HashMap<Hash, Instant>,
    ttl: Duration,
}

impl PendingFetches {
    pub fn new(ttl: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            ttl,
        }
    }

    fn purge_expired(&mut self, now: Instant) {
        self.pending.retain(|_, at| now.duration_since(*at) < self.ttl);
    }

    /// Returns `true` when a new fetch should be issued for `hash`.
    pub fn request(&mut self, hash: Hash) -> bool {
        let now = Instant::now();
        self.purge_expired(now);
        if self.pending.contains_key(&hash) {
            return false;
        }
        self.pending.insert(hash, now);
        true
    }

    pub fn complete(&mut self, hash: &Hash) {
        self.pending.remove(hash);
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.pending.contains_key(hash)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount, OutPoint, TxIn, TxOut};

    #[test]
    fn empty_compact_reconstructs() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1,
            bits: 0,
            nonce: 0,
            tx_root: Hash::ZERO,
        };
        let block = reconstruct_compact_block(header.clone(), &[], |_| None).unwrap();
        assert!(block.transactions.is_empty());
        assert_eq!(block.id(), header.hash());
    }

    #[test]
    fn reconstructs_from_short_ids() {
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: Address([1u8; 20]),
            }],
            7,
        );
        let sid = tx_short_id(&tx.tx_id());
        let header = BlockHeader {
            version: 1,
            parents: vec![],
            timestamp_ms: 2,
            bits: 0,
            nonce: 0,
            tx_root: Block::compute_tx_root(std::slice::from_ref(&tx)),
        };
        let pool_tx = tx.clone();
        let block = reconstruct_compact_block(header, &[sid], |id| {
            if id == &sid {
                Some(pool_tx.clone())
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(block.transactions.len(), 1);
        assert_eq!(block.transactions[0].nonce, 7);
    }

    #[test]
    fn pending_fetches_dedupes_until_ttl() {
        let mut pending = PendingFetches::new(Duration::from_secs(60));
        let h = Hash::hash_bytes(b"block");
        assert!(pending.request(h));
        assert!(!pending.request(h));
        pending.complete(&h);
        assert!(pending.request(h));
    }
}
