//! Library-facing partition stress coverage (CI-friendly subset of ghostdag_fuzzer).

use agora_consensus::{Dag, Ghostdag, GhostdagConfig};
use agora_types::Hash;

#[test]
fn partitioned_merges_are_deterministic_across_seeds() {
    for seed in [1u32, 7, 99, 1234, 9001] {
        let tip = build_partitioned_dag(seed);
        let mut a = Ghostdag::new(GhostdagConfig { k: 3 });
        let mut b = Ghostdag::new(GhostdagConfig { k: 3 });
        let order_a = a.order_past(&tip.0, tip.1).expect("order a");
        let order_b = b.order_past(&tip.0, tip.1).expect("order b");
        assert_eq!(order_a, order_b, "seed={seed}");
        assert!(!order_a.is_empty());
    }
}

fn build_partitioned_dag(seed: u32) -> (Dag, Hash) {
    let mut dag = Dag::new();
    let genesis = h(seed, 0, 0);
    dag.insert(genesis, vec![]).unwrap();

    let mut left = genesis;
    for i in 1..=8 {
        let block = h(seed, 1, i);
        dag.insert(block, vec![left]).unwrap();
        left = block;
    }
    let mut right = genesis;
    for i in 1..=8 {
        let block = h(seed, 2, i);
        dag.insert(block, vec![right]).unwrap();
        right = block;
    }
    let merge = h(seed, 3, 1);
    dag.insert(merge, vec![left, right]).unwrap();
    (dag, merge)
}

fn h(tag: u32, lane: u32, index: u32) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[0..4].copy_from_slice(&tag.to_le_bytes());
    bytes[4..8].copy_from_slice(&lane.to_le_bytes());
    bytes[8..12].copy_from_slice(&index.to_le_bytes());
    bytes[31] = (lane ^ index ^ tag) as u8;
    Hash(bytes)
}
