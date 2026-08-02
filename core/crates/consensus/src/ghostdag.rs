use std::collections::{HashMap, HashSet};

use agora_types::Hash;

use crate::dag::Dag;
use crate::ConsensusError;

/// Tunables for GHOSTDAG blue-set selection.
#[derive(Debug, Clone)]
pub struct GhostdagConfig {
    /// Maximum anticone size allowed inside the blue set (PHANTOM/GHOSTDAG `k`).
    pub k: u32,
}

impl Default for GhostdagConfig {
    fn default() -> Self {
        Self { k: 18 }
    }
}

/// Per-block GHOSTDAG output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostdagData {
    pub selected_parent: Option<Hash>,
    pub blue_score: u64,
    pub is_blue_in_tip_view: bool,
}

/// Block after GHOSTDAG has assigned relative order / color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedBlock {
    pub hash: Hash,
    pub blue_score: u64,
    pub is_blue: bool,
}

#[derive(Debug, Clone)]
struct BlockColoring {
    selected_parent: Option<Hash>,
    /// Blue blocks in the past of this block (including itself when it is blue in its own view).
    blues: HashSet<Hash>,
    blue_score: u64,
}

/// GHOSTDAG engine: greedy blue-set inheritance over an in-memory DAG.
#[derive(Debug, Default)]
pub struct Ghostdag {
    config: GhostdagConfig,
    coloring: HashMap<Hash, BlockColoring>,
}

impl Ghostdag {
    pub fn new(config: GhostdagConfig) -> Self {
        Self {
            config,
            coloring: HashMap::new(),
        }
    }

    pub fn config(&self) -> &GhostdagConfig {
        &self.config
    }

    pub fn blue_score(&self, hash: &Hash) -> Option<u64> {
        self.coloring.get(hash).map(|c| c.blue_score)
    }

    pub fn selected_parent(&self, hash: &Hash) -> Option<Hash> {
        self.coloring.get(hash).and_then(|c| c.selected_parent)
    }

    /// Color `block` given a DAG that already contains it and all ancestors.
    pub fn add_block(&mut self, dag: &Dag, block: Hash) -> Result<GhostdagData, ConsensusError> {
        if let Some(existing) = self.coloring.get(&block) {
            return Ok(GhostdagData {
                selected_parent: existing.selected_parent,
                blue_score: existing.blue_score,
                is_blue_in_tip_view: existing.blues.contains(&block),
            });
        }

        let parents = dag.parents_of(&block)?;
        if parents.is_empty() {
            let mut blues = HashSet::new();
            blues.insert(block);
            let coloring = BlockColoring {
                selected_parent: None,
                blue_score: 1,
                blues,
            };
            let out = GhostdagData {
                selected_parent: None,
                blue_score: 1,
                is_blue_in_tip_view: true,
            };
            self.coloring.insert(block, coloring);
            return Ok(out);
        }

        // Ensure parents are colored first (supports out-of-order calls if DAG is complete).
        for parent in parents {
            if !self.coloring.contains_key(parent) {
                self.add_block(dag, *parent)?;
            }
        }

        let selected_parent = parents
            .iter()
            .copied()
            .max_by(|a, b| {
                let sa = self.coloring[a].blue_score;
                let sb = self.coloring[b].blue_score;
                sa.cmp(&sb).then_with(|| a.as_bytes().cmp(b.as_bytes()))
            })
            .expect("parents non-empty");

        let mut blues = self.coloring[&selected_parent].blues.clone();
        blues.insert(selected_parent);

        let past_b = dag.past_closure(block)?;
        let past_sp = dag.past_closure(selected_parent)?;
        let mut merge_set: Vec<Hash> = past_b
            .into_iter()
            .filter(|h| *h != block && *h != selected_parent && !past_sp.contains(h))
            .collect();

        // Process merge-set in DAG insertion order for determinism.
        let order_index: HashMap<Hash, usize> = dag
            .blocks_in_insert_order()
            .iter()
            .copied()
            .enumerate()
            .map(|(i, h)| (h, i))
            .collect();
        merge_set.sort_by_key(|h| order_index.get(h).copied().unwrap_or(usize::MAX));

        for candidate in merge_set {
            let anticone = dag.anticone(candidate)?;
            let blue_anticone = anticone.intersection(&blues).count() as u32;
            if blue_anticone <= self.config.k {
                blues.insert(candidate);
            }
        }

        // The new tip is blue in its own view by construction of the selected chain.
        blues.insert(block);
        let blue_score = blues.len() as u64;
        let coloring = BlockColoring {
            selected_parent: Some(selected_parent),
            blues: blues.clone(),
            blue_score,
        };
        self.coloring.insert(block, coloring);

        Ok(GhostdagData {
            selected_parent: Some(selected_parent),
            blue_score,
            is_blue_in_tip_view: true,
        })
    }

    /// Color every block in insert order, then return a total order for a tip's past.
    pub fn order_past(&mut self, dag: &Dag, tip: Hash) -> Result<Vec<OrderedBlock>, ConsensusError> {
        for hash in dag.blocks_in_insert_order() {
            self.add_block(dag, *hash)?;
        }
        let past = dag.past_closure(tip)?;
        let tip_blues = &self
            .coloring
            .get(&tip)
            .ok_or_else(|| ConsensusError::MissingBlock(tip.to_hex()))?
            .blues;

        let mut ordered: Vec<OrderedBlock> = past
            .into_iter()
            .map(|hash| OrderedBlock {
                hash,
                blue_score: self.coloring[&hash].blue_score,
                is_blue: tip_blues.contains(&hash),
            })
            .collect();

        // Blues first by ascending blue_score, then reds; tie-break by hash bytes.
        ordered.sort_by(|a, b| {
            b.is_blue
                .cmp(&a.is_blue)
                .then_with(|| a.blue_score.cmp(&b.blue_score))
                .then_with(|| a.hash.as_bytes().cmp(b.hash.as_bytes()))
        });
        Ok(ordered)
    }

    /// Backward-compatible helper: insert tip with parents into a temporary view is not done here.
    /// Prefer [`Self::order_past`] with an explicit [`Dag`].
    pub fn order_tip(&self, tip: Hash, parents: &[Hash]) -> Result<Vec<OrderedBlock>, ConsensusError> {
        let mut ordered = Vec::with_capacity(parents.len() + 1);
        for (i, parent) in parents.iter().enumerate() {
            ordered.push(OrderedBlock {
                hash: *parent,
                blue_score: i as u64,
                is_blue: true,
            });
        }
        ordered.push(OrderedBlock {
            hash: tip,
            blue_score: parents.len() as u64,
            is_blue: true,
        });
        Ok(ordered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> Hash {
        let mut bytes = [0u8; 32];
        bytes[31] = byte;
        Hash(bytes)
    }

    #[test]
    fn chain_is_entirely_blue() {
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        dag.insert(h(1), vec![h(0)]).unwrap();
        dag.insert(h(2), vec![h(1)]).unwrap();

        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 18 });
        let ordered = ghostdag.order_past(&dag, h(2)).unwrap();
        assert_eq!(ordered.len(), 3);
        assert!(ordered.iter().all(|b| b.is_blue));
        assert_eq!(ghostdag.blue_score(&h(2)), Some(3));
    }

    #[test]
    fn parallel_blocks_both_blue_when_within_k() {
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        // Two parallel children of genesis.
        dag.insert(h(1), vec![h(0)]).unwrap();
        dag.insert(h(2), vec![h(0)]).unwrap();
        // Merge tip.
        dag.insert(h(3), vec![h(1), h(2)]).unwrap();

        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 18 });
        let data = ghostdag.add_block(&dag, h(3)).unwrap();
        assert!(data.blue_score >= 4);
        let ordered = ghostdag.order_past(&dag, h(3)).unwrap();
        let blues = ordered.iter().filter(|b| b.is_blue).count();
        assert_eq!(blues, 4);
    }

    #[test]
    fn k_zero_colors_non_selected_merge_red() {
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        dag.insert(h(1), vec![h(0)]).unwrap();
        dag.insert(h(2), vec![h(0)]).unwrap();
        dag.insert(h(3), vec![h(1), h(2)]).unwrap();

        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 0 });
        let ordered = ghostdag.order_past(&dag, h(3)).unwrap();
        let selected = ghostdag.coloring[&h(3)].selected_parent.unwrap();
        let non_selected = if selected == h(1) { h(2) } else { h(1) };
        let non_selected_block = ordered.iter().find(|b| b.hash == non_selected).unwrap();
        assert!(!non_selected_block.is_blue);
    }
}
