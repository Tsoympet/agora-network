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
3. `order_past(tip)` / `order_past_view(tip)` return tip-past blocks with blue/red flags.
4. `select_virtual_tip(tips)` picks the selected tip (max `blue_score`, hash tie-break).
5. `blue_order(tip)` = blues from that order — this is the UTXO apply sequence in `agora-node`.

`ChainState::admit_block` colors the DAG first, then reorgs live UTXO to blues of the virtual tip (see [`state-machine.md`](state-machine.md) Phase 28). Reds are stored but do not mutate UTXO yet.

### Admission limits (Phase 29)

`ConsensusLimits` (defaults):

| Limit | Default |
| --- | --- |
| max parents | 16 |
| max txs / block | 129 (1 coinbase + 128) |
| max tx inputs / outputs | 64 |
| max tx / block bytes | 100 KiB / 1 MiB |
| timestamp ahead | 60s; also ≥ max parent timestamp |
| coinbase maturity | 100 blue-score (genesis premine exempt) |

Templates trim tips to `max_block_parents` (blue_score desc) and clamp coinbase subsidy to remaining `max_supply − issued_supply`.

Synthetic DAG unit tests cover chains, parallel merges (`k` large ⇒ all blue), and `k = 0` red merges.

## DAA

`DaaConfig` targets sub-second block times (`target_block_time_ms`, default 1000).  
`Difficulty.level` maps 1:1 onto `BlockHeader.bits` (leading-zero requirement).

Wired in `agora-node` (`ChainState`):

1. Templates advertise `bits = difficulty.level`
2. Admission rejects blocks whose `bits` ≠ current difficulty
3. After admit, selected-parent spine samples (`timestamp_ms` + cumulative `work_from_bits`) feed `next_difficulty_weighted`
4. Level is persisted at `meta/daa_difficulty` (`u32` LE)

Canonical DAA / PoW / GHOSTDAG `k` live on `ChainParams` (`daa`, `pow_algorithm`, `ghostdag_k`, `bits`):

| Network | Initial `bits` | DAA `min_level` | PoW |
| --- | --- | --- | --- |
| `dev` | `AGORA_TEMPLATE_BITS` (default `1`) | from that value | env override OK |
| `testnet` | frozen `0` | `8` (post-genesis floor; genesis hash unchanged) | RandomX (env ignored) |
| `mainnet` | placeholder `16` (not bootable yet) | `8` | RandomX (env ignored) |

GHOSTDAG also tracks unit `blue_work` per blue tip; the DAA window overlays header-bits work so harder tips weigh spacing more.

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

## Dual-PoS finality (Trident Phase 3)

Additive gadget — does not replace GHOSTDAG/PoW admission:

- `quorum`, `finality`, `evidence` modules evaluate `FinalityCertificate` / reorg guards
- Spec: [`../consensus/HYBRID_POW_DUAL_POS.md`](../consensus/HYBRID_POW_DUAL_POS.md)
- Implementation notes: [`finality.md`](finality.md)

## Emission

`EmissionSchedule` owns reward math (initial subsidy + halving interval). Callers must not hardcode rewards elsewhere.

## Partition fuzzer

```bash
cargo run -p agora-consensus --bin ghostdag_fuzzer -- 128 42
```
