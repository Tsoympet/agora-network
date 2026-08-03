//! Compact-block reconstruction and IBD fetch bookkeeping.
//!
//! Peers gossip [`crate::NetworkMessage::CompactBlock`] / `BlockAnnounce`, then
//! request missing bodies with `GetBlock`. Empty short-id lists reconstruct
//! immediately (common for coinbase-only / empty templates).
//!
//! [`OrphanPool`] holds blocks whose parents are not yet local so multi-hop
//! IBD can fetch ancestors and re-admit children in order.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use agora_types::{Block, BlockHeader, Hash, Transaction};
use libp2p::PeerId;

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

/// Buffered block waiting on one or more missing parents.
#[derive(Debug, Clone)]
pub struct OrphanEntry {
    pub block: Block,
    pub seen_at: Instant,
    pub source_peer: Option<PeerId>,
}

/// In-memory orphan buffer for out-of-order / multi-hop IBD.
#[derive(Debug)]
pub struct OrphanPool {
    by_hash: HashMap<Hash, OrphanEntry>,
    /// missing parent → orphan block hashes waiting on it
    waiting_on: HashMap<Hash, HashSet<Hash>>,
    ttl: Duration,
    max_orphans: usize,
}

impl OrphanPool {
    pub fn new(ttl: Duration, max_orphans: usize) -> Self {
        Self {
            by_hash: HashMap::new(),
            waiting_on: HashMap::new(),
            ttl,
            max_orphans: max_orphans.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.by_hash.contains_key(hash)
    }

    fn purge_expired(&mut self, now: Instant) {
        let ttl = self.ttl;
        let expired: Vec<Hash> = self
            .by_hash
            .iter()
            .filter(|(_, e)| now.duration_since(e.seen_at) >= ttl)
            .map(|(h, _)| *h)
            .collect();
        for hash in expired {
            self.remove(&hash);
        }
    }

    fn remove_waiting_links(&mut self, orphan: &Hash) {
        self.waiting_on.retain(|_, waiters| {
            waiters.remove(orphan);
            !waiters.is_empty()
        });
    }

    /// Drop an orphan (and its waiting-on index entries).
    pub fn remove(&mut self, hash: &Hash) -> Option<OrphanEntry> {
        let entry = self.by_hash.remove(hash)?;
        self.remove_waiting_links(hash);
        Some(entry)
    }

    /// Park `block` until `missing_parents` are available. Returns `false` if not stored
    /// (already present, empty missing set, or eviction failed capacity policy).
    pub fn park(
        &mut self,
        block: Block,
        missing_parents: &[Hash],
        source_peer: Option<PeerId>,
    ) -> bool {
        let now = Instant::now();
        self.purge_expired(now);
        if missing_parents.is_empty() {
            return false;
        }
        let hash = block.id();
        if self.by_hash.contains_key(&hash) {
            // Refresh wait edges for newly reported missing parents.
            for parent in missing_parents {
                self.waiting_on.entry(*parent).or_default().insert(hash);
            }
            if let Some(entry) = self.by_hash.get_mut(&hash) {
                entry.seen_at = now;
                if source_peer.is_some() {
                    entry.source_peer = source_peer;
                }
            }
            return true;
        }
        while self.by_hash.len() >= self.max_orphans {
            // Evict oldest.
            let oldest = self
                .by_hash
                .iter()
                .min_by_key(|(_, e)| e.seen_at)
                .map(|(h, _)| *h);
            match oldest {
                Some(h) => {
                    self.remove(&h);
                }
                None => break,
            }
        }
        if self.by_hash.len() >= self.max_orphans {
            return false;
        }
        for parent in missing_parents {
            self.waiting_on.entry(*parent).or_default().insert(hash);
        }
        self.by_hash.insert(
            hash,
            OrphanEntry {
                block,
                seen_at: now,
                source_peer,
            },
        );
        true
    }

    /// After `parent` is admitted, take orphans that were waiting on it.
    /// Caller must re-check remaining parents before admitting.
    pub fn release_waiting_on(&mut self, parent: &Hash) -> Vec<Block> {
        let now = Instant::now();
        self.purge_expired(now);
        let Some(waiters) = self.waiting_on.remove(parent) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(waiters.len());
        for orphan_hash in waiters {
            // Drop this parent edge; keep the orphan until admitted or other parents arrive.
            if let Some(entry) = self.by_hash.remove(&orphan_hash) {
                self.remove_waiting_links(&orphan_hash);
                out.push(entry.block);
            }
        }
        out
    }

    /// Collect unique missing parents currently referenced by the pool (for refetch).
    pub fn missing_parents(&self) -> Vec<Hash> {
        self.waiting_on.keys().copied().collect()
    }
}

/// BFS helper: given a newly admitted hash, drain ready orphans via `try_admit`.
///
/// `try_admit` returns `Ok(id)` when admitted, `Err(Some(missing))` to re-park,
/// or `Err(None)` to drop (hard reject).
pub fn drain_orphans_after<F>(
    orphans: &mut OrphanPool,
    admitted: Hash,
    mut try_admit: F,
) -> Vec<Hash>
where
    F: FnMut(Block) -> Result<Hash, Option<Vec<Hash>>>,
{
    let mut admitted_ids = vec![admitted];
    let mut queue: VecDeque<Hash> = VecDeque::from([admitted]);
    while let Some(parent) = queue.pop_front() {
        for child in orphans.release_waiting_on(&parent) {
            match try_admit(child.clone()) {
                Ok(id) => {
                    admitted_ids.push(id);
                    queue.push_back(id);
                }
                Err(Some(missing)) => {
                    let _ = orphans.park(child, &missing, None);
                }
                Err(None) => {
                    // hard reject — drop
                }
            }
        }
    }
    admitted_ids
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

    fn coinbase_block(parent: Hash, nonce: u64) -> Block {
        let tx = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: Address([2u8; 20]),
            }],
            nonce,
        );
        let header = BlockHeader {
            version: 1,
            parents: vec![parent],
            timestamp_ms: nonce,
            bits: 0,
            nonce,
            tx_root: Block::compute_tx_root(std::slice::from_ref(&tx)),
        };
        Block {
            header,
            transactions: vec![tx],
        }
    }

    #[test]
    fn orphan_pool_parks_and_releases() {
        let mut pool = OrphanPool::new(Duration::from_secs(60), 16);
        let parent = Hash::hash_bytes(b"parent");
        let child = coinbase_block(parent, 1);
        let child_id = child.id();
        assert!(pool.park(child, &[parent], None));
        assert!(pool.contains(&child_id));
        assert_eq!(pool.missing_parents(), vec![parent]);

        let released = pool.release_waiting_on(&parent);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].id(), child_id);
        assert!(pool.is_empty());
    }

    #[test]
    fn orphan_pool_evicts_oldest_at_capacity() {
        let mut pool = OrphanPool::new(Duration::from_secs(60), 2);
        let p1 = Hash::hash_bytes(b"p1");
        let p2 = Hash::hash_bytes(b"p2");
        let p3 = Hash::hash_bytes(b"p3");
        let b1 = coinbase_block(p1, 1);
        let b2 = coinbase_block(p2, 2);
        let b3 = coinbase_block(p3, 3);
        let id1 = b1.id();
        assert!(pool.park(b1, &[p1], None));
        assert!(pool.park(b2, &[p2], None));
        assert!(pool.park(b3, &[p3], None));
        assert_eq!(pool.len(), 2);
        assert!(!pool.contains(&id1));
    }

    #[test]
    fn drain_orphans_admits_chain() {
        let mut pool = OrphanPool::new(Duration::from_secs(60), 16);
        let genesis = Hash::hash_bytes(b"genesis");
        let mid = coinbase_block(genesis, 1);
        let mid_id = mid.id();
        let tip = coinbase_block(mid_id, 2);
        let tip_id = tip.id();
        assert!(pool.park(tip, &[mid_id], None));
        assert!(pool.park(mid, &[genesis], None));

        let mut local = HashSet::from([genesis]);
        let admitted = drain_orphans_after(&mut pool, genesis, |block| {
            let missing: Vec<Hash> = block
                .header
                .parents
                .iter()
                .copied()
                .filter(|p| !local.contains(p))
                .collect();
            if !missing.is_empty() {
                return Err(Some(missing));
            }
            let id = block.id();
            local.insert(id);
            Ok(id)
        });
        assert!(admitted.contains(&mid_id));
        assert!(admitted.contains(&tip_id));
        assert!(pool.is_empty());
        assert!(local.contains(&tip_id));
    }
}
