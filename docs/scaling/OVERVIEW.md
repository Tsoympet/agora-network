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
| L2 OVL native PoW money (`sha256_leading_zero`, not L1 UTXO) | **implemented** |
| L2 `revm` executor (persistent state, CREATE, wired into runtime) | **implemented** |
| L2 `eth_*` subset on `agora-layers` | **implemented** (chainId / blockNumber / getBalance / getTransactionCount) |
| L3 lock/mint, burn/unlock, merkle proofs, transport | **implemented** |
| L3 DRC payments (`pay` + destination tag + path_pay) | **implemented** |
| L3 DRC district ledger | **implemented** |
| L4 naive + AMM + composite solvers, cancel, settle | **implemented** |
| `agora-layers` JSON-RPC binary | **implemented** (local operator runtime) |
| Public multi-chain deployment | **ops** — not claimed as live product |

## Run locally

```bash
AGORA_OVL_GENESIS_FILE=docs/genesis/ovolos.testnet.genesis.json \
AGORA_DRC_GENESIS_FILE=docs/genesis/drachma.testnet.genesis.json \
  cargo run -p agora-layers
# POST /rpc  {"method":"agora_layers_getInfo","params":{}}
# → ovl_genesis_hash / drc_genesis_hash / chain ids
```

Each mark has its own genesis: OVL on L2, DRC on L3 (TLT remains L1-only).

See also:

- [`TOKEN_ROLES.md`](TOKEN_ROLES.md) — TLT≈Bitcoin, OVL≈Ethereum, DRC≈XRP
- [`docs/core/ovolos-rollup.md`](../core/ovolos-rollup.md)
- [`docs/core/bridge-sdk.md`](../core/bridge-sdk.md)
- [`docs/core/intent-engine.md`](../core/intent-engine.md)
- [`docs/core/layers-runtime.md`](../core/layers-runtime.md)
