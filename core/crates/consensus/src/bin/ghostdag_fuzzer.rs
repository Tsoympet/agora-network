//! Stress-test GHOSTDAG against partitioned BlockDAG simulations.
//!
//! Usage:
//!   cargo run -p agora-consensus --bin ghostdag_fuzzer -- [iterations] [seed]
//!
//! Each iteration builds two partitions that mine in isolation, then merges
//! them through bridge tips — asserting deterministic ordering and blue-score
//! monotonicity along the selected-parent spine.

use std::collections::HashSet;
use std::env;
use std::process::ExitCode;

use agora_consensus::{Dag, Ghostdag, GhostdagConfig};
use agora_types::Hash;

fn main() -> ExitCode {
    let iterations: u32 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);
    let seed: u64 = env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xA60A_u64);

    let mut failures = 0u32;
    for i in 0..iterations {
        let iter_seed = seed.wrapping_add(u64::from(i).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if let Err(err) = run_partition_scenario(iter_seed, i) {
            eprintln!("iteration {i} failed: {err}");
            failures += 1;
        }
    }

    if failures == 0 {
        println!("ghostdag_fuzzer: {iterations} partitioned scenarios ok (seed={seed})");
        ExitCode::SUCCESS
    } else {
        eprintln!("ghostdag_fuzzer: {failures}/{iterations} failures");
        ExitCode::FAILURE
    }
}

fn run_partition_scenario(seed: u64, tag: u32) -> Result<(), String> {
    let mut rng = XorShift64::new(seed);
    let k = 1 + (rng.next_u32() % 8);
    let left_blocks = 4 + (rng.next_u32() % 12) as usize;
    let right_blocks = 4 + (rng.next_u32() % 12) as usize;
    let bridges = 1 + (rng.next_u32() % 3) as usize;

    let mut dag = Dag::new();
    let genesis = hash_from(tag, 0, 0);
    dag.insert(genesis, vec![]).map_err(|e| e.to_string())?;

    let mut left_tip = genesis;
    for n in 1..=left_blocks {
        let h = hash_from(tag, 1, n as u32);
        dag.insert(h, vec![left_tip]).map_err(|e| e.to_string())?;
        left_tip = h;
    }

    let mut right_tip = genesis;
    for n in 1..=right_blocks {
        let h = hash_from(tag, 2, n as u32);
        dag.insert(h, vec![right_tip]).map_err(|e| e.to_string())?;
        right_tip = h;
    }

    // Bridge tips reference both partitions — heals the network split.
    let mut merge_tip = left_tip;
    for n in 0..bridges {
        let parents = if n == 0 {
            vec![left_tip, right_tip]
        } else {
            // Additional bridges may also pull in a random earlier tip from either side.
            let mut parents = vec![merge_tip];
            if rng.next_u32() % 2 == 0 {
                parents.push(left_tip);
            } else {
                parents.push(right_tip);
            }
            parents.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            parents.dedup();
            parents
        };
        let h = hash_from(tag, 3, n as u32);
        dag.insert(h, parents).map_err(|e| e.to_string())?;
        merge_tip = h;
    }

    let mut ghostdag = Ghostdag::new(GhostdagConfig { k });
    let ordered = ghostdag
        .order_past(&dag, merge_tip)
        .map_err(|e| e.to_string())?;

    // Invariant: every block in tip past appears exactly once.
    let past = dag.past_closure(merge_tip).map_err(|e| e.to_string())?;
    if ordered.len() != past.len() {
        return Err(format!(
            "order len {} != past len {}",
            ordered.len(),
            past.len()
        ));
    }
    let mut seen = HashSet::new();
    for block in &ordered {
        if !past.contains(&block.hash) {
            return Err(format!(
                "ordered hash missing from past: {}",
                block.hash.to_hex()
            ));
        }
        if !seen.insert(block.hash) {
            return Err(format!("duplicate ordered hash: {}", block.hash.to_hex()));
        }
    }

    // Invariant: selected-parent spine has non-decreasing blue scores toward the tip.
    let mut cursor = merge_tip;
    let mut prev_score = ghostdag
        .blue_score(&cursor)
        .ok_or_else(|| "missing tip blue score".to_string())?;
    while let Some(parent) = ghostdag.selected_parent(&cursor) {
        let score = ghostdag
            .blue_score(&parent)
            .ok_or_else(|| "missing parent blue score".to_string())?;
        if score > prev_score {
            return Err(format!(
                "blue score increased walking toward genesis: {score} > {prev_score}"
            ));
        }
        prev_score = score;
        cursor = parent;
    }

    // Determinism: re-run coloring from a fresh engine.
    let mut ghostdag2 = Ghostdag::new(GhostdagConfig { k });
    let ordered2 = ghostdag2
        .order_past(&dag, merge_tip)
        .map_err(|e| e.to_string())?;
    if ordered != ordered2 {
        return Err("non-deterministic GHOSTDAG ordering".into());
    }

    Ok(())
}

fn hash_from(tag: u32, lane: u32, index: u32) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[0..4].copy_from_slice(&tag.to_le_bytes());
    bytes[4..8].copy_from_slice(&lane.to_le_bytes());
    bytes[8..12].copy_from_slice(&index.to_le_bytes());
    // Mix in a domain tag so lanes never collide with index encoding.
    bytes[31] = lane as u8 ^ index as u8 ^ tag as u8;
    Hash(bytes)
}

/// Tiny deterministic PRNG (no extra deps).
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
}
