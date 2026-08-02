# Agora Scaling Layers

| Layer | Crate | Role |
| --- | --- | --- |
| L2 | `agora-ovolos-rollup` | Ovolos optimistic rollup for EVM batches |
| L3 | `agora-bridge-sdk` | Bridge-in-a-Box for District Chains |
| L4 | `agora-intent-engine` | Intent-Engine for AI-driven orchestration |

```
Users / Agents
      │ intents
      ▼
Intent-Engine (L4) ──solvers──► Bridge-in-a-Box (L3) ──► District Chains
                                      │
                                      ▼
                              Ovolos Rollup (L2) ──batches──► Agora L1 BlockDAG
```

## Status

- L2 executor: audited `revm` bound via `RevmExecutor` (default feature)
- L3 messaging: merkle light-client proofs + `MessageTransport`
- L4: settles intents through Bridge-in-a-Box

See also:

- [`docs/core/ovolos-rollup.md`](../core/ovolos-rollup.md)
- [`docs/core/bridge-sdk.md`](../core/bridge-sdk.md)
- [`docs/core/intent-engine.md`](../core/intent-engine.md)
