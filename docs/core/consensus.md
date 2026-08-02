# Consensus (`agora-consensus`)

GHOSTDAG ordering, difficulty adjustment, PoW verification, and emission.

## GHOSTDAG

Agora orders a BlockDAG rather than a single chain. Parallel blocks are retained; GHOSTDAG selects a blue set (parameter `k`) and induces a total order for transaction conflict resolution.

### Algorithm (greedy)

1. Build / consult an in-memory `Dag` of block hash → parents.
2. For each block `B` in insert order:
   - Choose **selected parent** = parent with highest `blue_score` (hash tie-break).
   - Inherit the selected parent's blue set and add the selected parent.
   - Walk the **merge set** (`past(B) \ past(selected_parent)`); add a candidate if `|anticone(candidate) ∩ blues| ≤ k`.
   - Insert `B` into its blue set; `blue_score(B) = |blues|`.
3. `order_past(tip)` returns tip-past blocks with blue/red flags for conflict resolution.

Synthetic DAG unit tests cover chains, parallel merges (`k` large ⇒ all blue), and `k = 0` red merges.

## DAA

`DaaConfig` targets sub-second block times (`target_block_time_ms`, default 1000).  
`next_difficulty` adjusts a simple integer difficulty level from a timestamp window, clamped by `max_adjustment_factor`.

Production will key windows off blue-work; the scaffold uses timestamps to lock the API.

## PoW

| Algorithm | Target hardware | Hasher |
| --- | --- | --- |
| RandomX | CPU (`miner-sidecar`) | `Sha256PowHasher` stand-in until RandomX FFI |
| kHeavyHash | ASIC (`stratum-pool`) | `KHeavyHashPowHasher` → `agora-kheavyhash` |

Traits:

- `PowHasher` — compute digest for a `BlockHeader`
- `PowVerifier` — check digest + difficulty (`LeadingZeroPow` uses `header.bits` as leading-zero requirement)

### kHeavyHash (`agora-kheavyhash`)

Vendored from rusty-kaspa (`kaspa-hashes` / `kaspa-pow`, ISC). Pipeline:

1. Pre-PoW commitment = `SHA-256(borsh(header))` with `nonce = 0` and `timestamp_ms = 0`
2. Kaspa `PowHash::new(pre, timestamp).finalize_with_nonce(nonce)`
3. `Matrix::generate(pre).heavy_hash(...)` (includes final `KHeavyHash` cSHAKE domain)

## Emission

`EmissionSchedule` owns reward math (initial subsidy + halving interval). Callers must not hardcode rewards elsewhere.

## Partition fuzzer

```bash
cargo run -p agora-consensus --bin ghostdag_fuzzer -- 128 42
```

Simulates two isolated mining partitions that later merge through bridge tips, then asserts:

- ordered set == tip past (no dupes / omissions)
- selected-parent spine has non-increasing blue score toward genesis
- coloring is deterministic across fresh engines

CI also runs `tests/ghostdag_partition_fuzz.rs`.
