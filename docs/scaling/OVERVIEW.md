# Agora Scaling Layers

| Layer | Crate | Role |
| --- | --- | --- |
| L2 | `agora-ovolos-rollup` | Ovolos optimistic rollup for EVM batches + OVL gas ledger |
| L3 | `agora-bridge-sdk` | Bridge-in-a-Box for District Chains + DRC ledger |
| L4 | `agora-intent-engine` | Intent-Engine (bridge routes + same-district AMM) |
| Runtime | `agora-layers-runtime` / `agora-layers` | In-process composition + operator JSON-RPC |

```
Users / Agents
      │ intents
      ▼
Intent-Engine (L4) ──solvers──► Bridge-in-a-Box (L3) ──► District Chains
                                      │
                                      ▼
                              Ovolos Rollup (L2) ──batches / DA──► Agora L1 BlockDAG
```

## Status

| Piece | Status |
| --- | --- |
| L2 sequencer / challenge / finalize / revert | **implemented** (in-process) |
| L2 `BatchCommitment` DA blob | **implemented** (operator posts to L1 separately) |
| L2 OVL gas ledger (registry mark, not L1 UTXO) | **implemented** |
| L2 `revm` executor | **implemented** (default feature) |
| L3 lock/mint, burn/unlock, merkle proofs, transport | **implemented** |
| L3 DRC district ledger | **implemented** |
| L4 naive + AMM + composite solvers, cancel, settle | **implemented** |
| `agora-layers` JSON-RPC binary | **implemented** (local operator runtime) |
| Public multi-chain deployment | **ops** — not claimed as live product |

## Run locally

```bash
cargo run -p agora-layers
# AGORA_LAYERS_BIND=127.0.0.1:8555
# POST /rpc  {"method":"agora_layers_getInfo","params":{}}
```

See also:

- [`docs/core/ovolos-rollup.md`](../core/ovolos-rollup.md)
- [`docs/core/bridge-sdk.md`](../core/bridge-sdk.md)
- [`docs/core/intent-engine.md`](../core/intent-engine.md)
- [`docs/core/layers-runtime.md`](../core/layers-runtime.md)
