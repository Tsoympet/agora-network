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
`Difficulty.level` maps 1:1 onto `BlockHeader.bits` (leading-zero requirement).

Wired in `agora-node` (`ChainState`):

1. Templates advertise `bits = difficulty.level`
2. Admission rejects blocks whose `bits` ≠ current difficulty
3. After admit, timestamps along the selected-parent spine feed `next_difficulty`
4. Level is persisted at `meta/daa_difficulty` (`u32` LE)

`AGORA_TEMPLATE_BITS` sets the initial level (and `min_level = 0` when started at 0).  
Production will key windows off blue-work; the scaffold still uses timestamps.

## PoW

| Algorithm | Target hardware | Hasher |
| --- | --- | --- |
| RandomX | CPU (`miner-sidecar`) | `RandomXPowHasher` (`rust-randomx`, feature `randomx`) |
| kHeavyHash | ASIC (`stratum-pool`) | `KHeavyHashPowHasher` → `agora-kheavyhash` |

Traits:

- `PowHasher` — compute digest for a `BlockHeader`
- `PowVerifier` — check digest + difficulty (`LeadingZeroPow` uses `header.bits` as leading-zero requirement)

### RandomX

- Key = `SHA-256(borsh(header))` with `nonce = 0`
- Input = full `borsh(header)` including candidate nonce
- Feature `randomx` (default) binds official RandomX via `rust-randomx` (needs cmake + g++; see `.cargo/config.toml`)
- Without the feature, `RandomXPowHasher` falls back to SHA-256 so the workspace still compiles

### kHeavyHash (`agora-kheavyhash`)

Vendored from rusty-kaspa (`kaspa-hashes` / `kaspa-pow`, ISC). Pipeline:

1. Pre-PoW commitment = `SHA-256(borsh(header))` with `nonce = 0` and `timestamp_ms = 0`
2. Kaspa `PowHash::new(pre, timestamp).finalize_with_nonce(nonce)`
3. `Matrix::generate(pre).heavy_hash(...)` (includes final `KHeavyHash` cSHAKE domain)

### Block admission (`agora-node`)

`ChainState::admit_block` runs `LeadingZeroPow::verify`, persists the block, updates tips, then `Dag::insert` + `Ghostdag::add_block`. Called from:

- RPC `agora_submitBlock`
- Gossip `NetworkMessage::Block`

## Emission

`EmissionSchedule` owns reward math (initial subsidy + halving interval). Callers must not hardcode rewards elsewhere.

## Partition fuzzer

```bash
cargo run -p agora-consensus --bin ghostdag_fuzzer -- 128 42
```
