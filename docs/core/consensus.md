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

| Algorithm | Target hardware | Integration |
| --- | --- | --- |
| RandomX | CPU | `miner-sidecar` |
| kHeavyHash | ASIC | `infrastructure/stratum-pool` |

Verification is trait-based (`PowVerifier`). Until RandomX/kHeavyHash FFI lands, `LeadingZeroPow` treats `header.bits` as required leading zero bits on `SHA-256(borsh(header))`.

## Emission

`EmissionSchedule` owns reward math (initial subsidy + halving interval). Callers must not hardcode rewards elsewhere.
