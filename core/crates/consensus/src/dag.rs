use std::collections::{HashMap, HashSet, VecDeque};

use agora_types::Hash;

use crate::ConsensusError;

/// In-memory BlockDAG view used by GHOSTDAG (hash → parent hashes).
///
/// Production nodes will back this with the state-machine store; tests use synthetic graphs.
#[derive(Debug, Default, Clone)]
pub struct Dag {
    /// Insertion-ordered blocks for deterministic traversal ties.
    order: Vec<Hash>,
    parents: HashMap<Hash, Vec<Hash>>,
}

impl Dag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.parents.contains_key(hash)
    }

    pub fn parents_of(&self, hash: &Hash) -> Result<&[Hash], ConsensusError> {
        self.parents
            .get(hash)
            .map(|p| p.as_slice())
            .ok_or_else(|| ConsensusError::MissingBlock(hash.to_hex()))
    }

    /// Insert a block. Parents must already exist (except when inserting genesis with no parents).
    pub fn insert(&mut self, hash: Hash, parents: Vec<Hash>) -> Result<(), ConsensusError> {
        if self.contains(&hash) {
            return Ok(());
        }
        for parent in &parents {
            if !self.contains(parent) {
                return Err(ConsensusError::MissingBlock(parent.to_hex()));
            }
        }
        self.order.push(hash);
        self.parents.insert(hash, parents);
        Ok(())
    }

    pub fn tips(&self) -> Vec<Hash> {
        let mut referenced = HashSet::new();
        for parents in self.parents.values() {
            for p in parents {
                referenced.insert(*p);
            }
        }
        self.order
            .iter()
            .copied()
            .filter(|h| !referenced.contains(h))
            .collect()
    }

    /// All blocks in the past of `hash`, including itself (BFS over parents).
    pub fn past_closure(&self, hash: Hash) -> Result<HashSet<Hash>, ConsensusError> {
        if !self.contains(&hash) {
            return Err(ConsensusError::MissingBlock(hash.to_hex()));
        }
        let mut seen = HashSet::new();
        let mut q = VecDeque::from([hash]);
        while let Some(current) = q.pop_front() {
            if !seen.insert(current) {
                continue;
            }
            for parent in self.parents_of(&current)? {
                q.push_back(*parent);
            }
        }
        Ok(seen)
    }

    /// Blocks that are neither in past(A) nor past(B) relative to the full DAG — anticone of `hash`.
    pub fn anticone(&self, hash: Hash) -> Result<HashSet<Hash>, ConsensusError> {
        let past = self.past_closure(hash)?;
        let mut future = HashSet::new();
        // Future = blocks that include `hash` in their past (excluding hash itself).
        for candidate in &self.order {
            if *candidate == hash {
                continue;
            }
            let candidate_past = self.past_closure(*candidate)?;
            if candidate_past.contains(&hash) {
                future.insert(*candidate);
            }
        }
        let mut anticone = HashSet::new();
        for candidate in &self.order {
            if *candidate == hash {
                continue;
            }
            if !past.contains(candidate) && !future.contains(candidate) {
                anticone.insert(*candidate);
            }
        }
        Ok(anticone)
    }

    pub fn blocks_in_insert_order(&self) -> &[Hash] {
        &self.order
    }
}
