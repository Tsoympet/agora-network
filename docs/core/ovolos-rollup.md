# Ovolos Rollup (`agora-ovolos-rollup`)

Optimistic L2 for EVM smart-contract scaling on Agora.

## Flow

1. Sequencer builds a `Batch` (ordered EVM txs + prev/post state roots)
2. `OvolosRollup::submit_batch` re-executes via `EvmExecutor` and accepts only matching roots
3. Batch stays `Pending` for `challenge_window_ms`
4. Watchers may submit a `FraudProof`; successful challenges mark `Challenged`
5. `finalize_due` promotes unchallenged batches to `Finalized`

## EVM execution

`EvmExecutor` is intentionally pluggable. `StubEvmExecutor` provides a deterministic pseudo-root for sequencing tests. Production binds an audited EVM (e.g. `revm`) behind the same trait — no custom VM logic in-repo.
