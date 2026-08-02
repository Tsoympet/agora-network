# Ovolos Rollup (`agora-ovolos-rollup`)

Optimistic L2 for EVM smart-contract scaling on Agora.

## Flow

1. Sequencer builds a `Batch` (ordered EVM txs + prev/post state roots)
2. `OvolosRollup::submit_batch` re-executes via `EvmExecutor` and accepts only matching roots
3. Batch stays `Pending` for `challenge_window_ms`
4. Watchers may submit a `FraudProof`; successful challenges mark `Challenged`
5. `finalize_due` promotes unchallenged batches to `Finalized`

## EVM execution

| Executor | Feature | Role |
| --- | --- | --- |
| `StubEvmExecutor` | always | Deterministic pseudo-root for sequencing unit tests |
| `RevmExecutor` | `revm` (default) | Audited [`revm`](https://crates.io/crates/revm) binding |

`RevmExecutor` decodes compact transfers `to(20) || value(32) || calldata?`, runs them through mainnet `revm`, and hashes the resulting account cache into a post-state root. Use `encode_transfer` helpers in tests.
