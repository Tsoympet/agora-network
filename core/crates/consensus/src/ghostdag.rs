use std::collections::{HashMap, HashSet};

use agora_types::Hash;
use borsh::{BorshDeserialize, BorshSerialize};

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
    /// Cumulative work of accepted blues in this block's view
    /// (selected-parent blues + merge-set blues + self).
    ///
    /// On the node each block contributes `work_from_bits(header.bits)` via
    /// [`Ghostdag::add_block_with_work`].
    pub blue_work: u128,
    pub is_blue_in_tip_view: bool,
}

/// Durable coloring snapshot (compact merge-set form).
///
/// Stores **mergeset blues** (not the full past blue set) so persistent GHOSTDAG
/// metadata stays O(mergeset) per block. Full past blues are reconstructed by
/// walking the selected-parent chain on hydrate.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GhostdagSnapshot {
    pub selected_parent: Option<Hash>,
    pub blue_score: u64,
    pub blue_work: u128,
    /// This block's own PoW contribution.
    pub block_work: u128,
    /// Blues accepted from the merge set (excludes selected-parent past and self).
    pub mergeset_blues: Vec<Hash>,
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
    ///
    /// Kept in memory for anticone checks; durable snapshots store only `mergeset_blues`.
    blues: HashSet<Hash>,
    /// Blues newly accepted from the merge set (compact durable form).
    mergeset_blues: Vec<Hash>,
    /// This block's own work contribution.
    block_work: u128,
    blue_score: u64,
    /// Cumulative work of **all accepted blues** in this block's view
    /// (`SP.blue_work + Σ mergeset_blue.block_work + self.block_work`).
    blue_work: u128,
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

    pub fn blue_work(&self, hash: &Hash) -> Option<u128> {
        self.coloring.get(hash).map(|c| c.blue_work)
    }

    pub fn selected_parent(&self, hash: &Hash) -> Option<Hash> {
        self.coloring.get(hash).and_then(|c| c.selected_parent)
    }

    /// Drop coloring for `hash` (admission rollback). Does not rewrite ancestors.
    pub fn remove(&mut self, hash: &Hash) {
        self.coloring.remove(hash);
    }

    /// Export a durable coloring snapshot for `hash`.
    pub fn snapshot(&self, hash: &Hash) -> Option<GhostdagSnapshot> {
        let c = self.coloring.get(hash)?;
        let mut mergeset_blues = c.mergeset_blues.clone();
        mergeset_blues.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        Some(GhostdagSnapshot {
            selected_parent: c.selected_parent,
            blue_score: c.blue_score,
            blue_work: c.blue_work,
            block_work: c.block_work,
            mergeset_blues,
        })
    }

    /// Whether `block` is blue in the current virtual tip's view.
    pub fn is_blue_in_view(&self, tip: &Hash, block: &Hash) -> bool {
        self.coloring
            .get(tip)
            .map(|c| c.blues.contains(block))
            .unwrap_or(false)
    }

    /// Compare two colored blocks for selected-parent / tip ranking.
    fn rank_key(
        a: &BlockColoring,
        a_hash: &Hash,
        b: &BlockColoring,
        b_hash: &Hash,
    ) -> std::cmp::Ordering {
        a.blue_work
            .cmp(&b.blue_work)
            .then_with(|| a.blue_score.cmp(&b.blue_score))
            .then_with(|| a_hash.as_bytes().cmp(b_hash.as_bytes()))
    }

    /// Hydrate coloring from a durable snapshot without recomputing GHOSTDAG.
    ///
    /// Requires the selected parent (if any) to already be hydrated so the full
    /// blue set can be reconstructed from compact `mergeset_blues`.
    pub fn import_snapshot(&mut self, hash: Hash, snap: GhostdagSnapshot) -> GhostdagData {
        let mut blues: HashSet<Hash> = HashSet::new();
        if let Some(sp) = snap.selected_parent {
            if let Some(parent) = self.coloring.get(&sp) {
                blues.extend(parent.blues.iter().copied());
                blues.insert(sp);
            }
        }
        for h in &snap.mergeset_blues {
            blues.insert(*h);
        }
        blues.insert(hash);
        let is_blue = blues.contains(&hash);
        let data = GhostdagData {
            selected_parent: snap.selected_parent,
            blue_score: snap.blue_score,
            blue_work: snap.blue_work,
            is_blue_in_tip_view: is_blue,
        };
        self.coloring.insert(
            hash,
            BlockColoring {
                selected_parent: snap.selected_parent,
                blues,
                mergeset_blues: snap.mergeset_blues,
                block_work: snap.block_work.max(1),
                blue_score: snap.blue_score,
                blue_work: snap.blue_work,
            },
        );
        data
    }

    /// Color `block` with unit work (1 per block).
    ///
    /// Prefer [`Self::add_block_with_work`] on the node so accumulated `blue_work`
    /// reflects real proof-of-work (`work_from_bits`) for tip selection.
    pub fn add_block(&mut self, dag: &Dag, block: Hash) -> Result<GhostdagData, ConsensusError> {
        self.add_block_with_work(dag, block, 1)
    }

    /// Color `block` given a DAG that already contains it and all ancestors.
    ///
    /// `block_work` is this block's own PoW contribution (e.g. `work_from_bits(bits)`).
    /// Cumulative `blue_work` includes the selected parent's blue work **plus** the work
    /// of every newly accepted merge-set blue **plus** this block — i.e. accepted blue
    /// DAG work, not merely selected-chain work.
    ///
    /// Selected parent is chosen by the same ranking as [`Self::select_virtual_tip`]
    /// (`blue_work`, then `blue_score`, then hash).
    pub fn add_block_with_work(
        &mut self,
        dag: &Dag,
        block: Hash,
        block_work: u128,
    ) -> Result<GhostdagData, ConsensusError> {
        if let Some(existing) = self.coloring.get(&block) {
            return Ok(GhostdagData {
                selected_parent: existing.selected_parent,
                blue_score: existing.blue_score,
                blue_work: existing.blue_work,
                is_blue_in_tip_view: existing.blues.contains(&block),
            });
        }

        let block_work = block_work.max(1);
        let parents = dag.parents_of(&block)?;
        if parents.is_empty() {
            let mut blues = HashSet::new();
            blues.insert(block);
            let coloring = BlockColoring {
                selected_parent: None,
                blue_score: 1,
                blue_work: block_work,
                blues,
                mergeset_blues: Vec::new(),
                block_work,
            };
            let out = GhostdagData {
                selected_parent: None,
                blue_score: 1,
                blue_work: block_work,
                is_blue_in_tip_view: true,
            };
            self.coloring.insert(block, coloring);
            return Ok(out);
        }

        for parent in parents {
            if !self.coloring.contains_key(parent) {
                self.add_block(dag, *parent)?;
            }
        }

        let selected_parent = parents
            .iter()
            .copied()
            .max_by(|a, b| Self::rank_key(&self.coloring[a], a, &self.coloring[b], b))
            .expect("parents non-empty");

        let mut blues = self.coloring[&selected_parent].blues.clone();
        blues.insert(selected_parent);

        let past_b = dag.past_closure(block)?;
        let past_sp = dag.past_closure(selected_parent)?;
        let mut merge_set: Vec<Hash> = past_b
            .into_iter()
            .filter(|h| *h != block && *h != selected_parent && !past_sp.contains(h))
            .collect();

        merge_set.sort_by(|a, b| {
            let sa = self.coloring.get(a).map(|c| c.blue_score).unwrap_or(0);
            let sb = self.coloring.get(b).map(|c| c.blue_score).unwrap_or(0);
            sa.cmp(&sb).then_with(|| a.as_bytes().cmp(b.as_bytes()))
        });

        let mut mergeset_blues = Vec::new();
        let mut mergeset_work = 0u128;
        for candidate in merge_set {
            let anticone = dag.anticone(candidate)?;
            let blue_anticone = anticone.intersection(&blues).count() as u32;
            if blue_anticone <= self.config.k {
                blues.insert(candidate);
                mergeset_blues.push(candidate);
                let w = self
                    .coloring
                    .get(&candidate)
                    .map(|c| c.block_work)
                    .unwrap_or(1);
                mergeset_work = mergeset_work.saturating_add(w);
            }
        }

        blues.insert(block);
        let blue_score = blues.len() as u64;
        let parent_work = self.coloring[&selected_parent].blue_work;
        let blue_work = parent_work
            .saturating_add(mergeset_work)
            .saturating_add(block_work);
        mergeset_blues.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let coloring = BlockColoring {
            selected_parent: Some(selected_parent),
            blues: blues.clone(),
            mergeset_blues,
            block_work,
            blue_score,
            blue_work,
        };
        self.coloring.insert(block, coloring);

        Ok(GhostdagData {
            selected_parent: Some(selected_parent),
            blue_score,
            blue_work,
            is_blue_in_tip_view: true,
        })
    }

    /// Pick the virtual tip among `tips`: highest accumulated **blue work**, then blue
    /// score, then hash bytes.
    pub fn select_virtual_tip(&self, tips: &[Hash]) -> Option<Hash> {
        tips.iter()
            .copied()
            .filter(|h| self.coloring.contains_key(h))
            .max_by(|a, b| Self::rank_key(&self.coloring[a], a, &self.coloring[b], b))
    }

    /// Choose the selected parent among `parents` using work-then-score ranking.
    pub fn select_parent_among(&self, parents: &[Hash]) -> Option<Hash> {
        parents
            .iter()
            .copied()
            .filter(|h| self.coloring.contains_key(h))
            .max_by(|a, b| Self::rank_key(&self.coloring[a], a, &self.coloring[b], b))
    }

    /// Color every block in insert order, then return a total order for a tip's past.
    pub fn order_past(
        &mut self,
        dag: &Dag,
        tip: Hash,
    ) -> Result<Vec<OrderedBlock>, ConsensusError> {
        for hash in dag.blocks_in_insert_order() {
            self.add_block(dag, *hash)?;
        }
        self.order_past_view(dag, tip)
    }

    /// Total order for `tip`'s past assuming the DAG is already fully colored.
    pub fn order_past_view(
        &self,
        dag: &Dag,
        tip: Hash,
    ) -> Result<Vec<OrderedBlock>, ConsensusError> {
        let past = dag.past_closure(tip)?;
        let tip_blues = &self
            .coloring
            .get(&tip)
            .ok_or_else(|| ConsensusError::MissingBlock(tip.to_hex()))?
            .blues;

        let mut ordered: Vec<OrderedBlock> = past
            .into_iter()
            .map(|hash| {
                let blue_score = self.coloring.get(&hash).map(|c| c.blue_score).unwrap_or(0);
                OrderedBlock {
                    hash,
                    blue_score,
                    is_blue: tip_blues.contains(&hash),
                }
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

    /// Blue hashes from [`order_past_view`] in apply order (ascending blue_score).
    pub fn blue_order(&self, dag: &Dag, tip: Hash) -> Result<Vec<Hash>, ConsensusError> {
        Ok(self
            .order_past_view(dag, tip)?
            .into_iter()
            .filter(|b| b.is_blue)
            .map(|b| b.hash)
            .collect())
    }

    /// Backward-compatible helper: insert tip with parents into a temporary view is not done here.
    /// Prefer [`Self::order_past`] with an explicit [`Dag`].
    pub fn order_tip(
        &self,
        tip: Hash,
        parents: &[Hash],
    ) -> Result<Vec<OrderedBlock>, ConsensusError> {
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

    /// Build the same logical DAG under many valid insertion permutations and assert
    /// GHOSTDAG coloring + ordering are identical (arrival-order independence).
    #[test]
    fn coloring_is_independent_of_insertion_order() {
        // Diamond + extra parallel tips: genesis -> {1,2,3} -> 4(1,2) , 5(2,3) , tip 6(4,5,1)
        let edges: Vec<(u8, Vec<u8>)> = vec![
            (0, vec![]),
            (1, vec![0]),
            (2, vec![0]),
            (3, vec![0]),
            (4, vec![1, 2]),
            (5, vec![2, 3]),
            (6, vec![4, 5, 1]),
        ];

        // A few distinct but all-valid topological insertion orders.
        let orders: [&[u8]; 4] = [
            &[0, 1, 2, 3, 4, 5, 6],
            &[0, 3, 2, 1, 5, 4, 6],
            &[0, 2, 1, 3, 4, 5, 6],
            &[0, 1, 3, 2, 5, 4, 6],
        ];

        let mut reference: Option<Vec<OrderedBlock>> = None;
        let mut reference_bluework: Option<u128> = None;
        for order in orders {
            let mut dag = Dag::new();
            for &b in order {
                let parents = edges
                    .iter()
                    .find(|(h, _)| *h == b)
                    .map(|(_, p)| p.iter().map(|x| h(*x)).collect::<Vec<_>>())
                    .unwrap();
                dag.insert(h(b), parents).unwrap();
            }
            let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 3 });
            let ordered = ghostdag.order_past(&dag, h(6)).unwrap();
            let bluework = ghostdag.blue_work(&h(6)).unwrap();
            match (&reference, reference_bluework) {
                (None, _) => {
                    reference = Some(ordered);
                    reference_bluework = Some(bluework);
                }
                (Some(expected), Some(expected_work)) => {
                    assert_eq!(&ordered, expected, "ordering diverged for order {order:?}");
                    assert_eq!(bluework, expected_work, "blue_work diverged for {order:?}");
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn virtual_tip_prefers_higher_accumulated_work() {
        // genesis -> a (1 low-work block) vs genesis -> b1 -> b2 (2 low-work blocks).
        // With work weighting, a single high-work tip outranks a longer low-work chain.
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        dag.insert(h(1), vec![h(0)]).unwrap(); // heavy tip
        dag.insert(h(2), vec![h(0)]).unwrap();
        dag.insert(h(3), vec![h(2)]).unwrap(); // longer, but light

        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 18 });
        ghostdag.add_block_with_work(&dag, h(0), 1).unwrap();
        ghostdag.add_block_with_work(&dag, h(1), 1_000_000).unwrap();
        ghostdag.add_block_with_work(&dag, h(2), 1).unwrap();
        ghostdag.add_block_with_work(&dag, h(3), 1).unwrap();

        // h(3) has higher blue_score (3) but h(1) has far higher blue_work.
        assert!(ghostdag.blue_score(&h(3)).unwrap() > ghostdag.blue_score(&h(1)).unwrap());
        assert_eq!(ghostdag.select_virtual_tip(&[h(1), h(3)]), Some(h(1)));
    }

    #[test]
    fn selected_parent_prefers_work_not_score() {
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        dag.insert(h(1), vec![h(0)]).unwrap(); // heavy
        dag.insert(h(2), vec![h(0)]).unwrap();
        dag.insert(h(3), vec![h(2)]).unwrap(); // longer light chain tip
        dag.insert(h(4), vec![h(1), h(3)]).unwrap();

        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 18 });
        ghostdag.add_block_with_work(&dag, h(0), 1).unwrap();
        ghostdag.add_block_with_work(&dag, h(1), 1_000_000).unwrap();
        ghostdag.add_block_with_work(&dag, h(2), 1).unwrap();
        ghostdag.add_block_with_work(&dag, h(3), 1).unwrap();
        let data = ghostdag.add_block_with_work(&dag, h(4), 1).unwrap();
        assert_eq!(data.selected_parent, Some(h(1)));
    }

    #[test]
    fn blue_work_includes_mergeset_blues() {
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        dag.insert(h(1), vec![h(0)]).unwrap();
        dag.insert(h(2), vec![h(0)]).unwrap();
        dag.insert(h(3), vec![h(1), h(2)]).unwrap();

        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 18 });
        ghostdag.add_block_with_work(&dag, h(0), 10).unwrap();
        ghostdag.add_block_with_work(&dag, h(1), 100).unwrap();
        ghostdag.add_block_with_work(&dag, h(2), 50).unwrap();
        let data = ghostdag.add_block_with_work(&dag, h(3), 7).unwrap();
        // SP is h(1) (higher work). blues = {0,1,2,3}; work = 10+100+50+7.
        assert_eq!(data.selected_parent, Some(h(1)));
        assert_eq!(data.blue_work, 10 + 100 + 50 + 7);
    }

    #[test]
    fn select_virtual_tip_picks_highest_blue_score() {
        let mut dag = Dag::new();
        dag.insert(h(0), vec![]).unwrap();
        dag.insert(h(1), vec![h(0)]).unwrap();
        dag.insert(h(2), vec![h(0)]).unwrap();
        let mut ghostdag = Ghostdag::new(GhostdagConfig { k: 18 });
        ghostdag.add_block(&dag, h(0)).unwrap();
        ghostdag.add_block(&dag, h(1)).unwrap();
        ghostdag.add_block(&dag, h(2)).unwrap();
        // Both children have blue_score 2; higher hash wins (h(2) > h(1)).
        assert_eq!(ghostdag.select_virtual_tip(&[h(1), h(2)]), Some(h(2)));
        let view = ghostdag.order_past_view(&dag, h(2)).unwrap();
        let mut again = Ghostdag::new(GhostdagConfig { k: 18 });
        let via_mut = again.order_past(&dag, h(2)).unwrap();
        assert_eq!(view, via_mut);
    }
}
