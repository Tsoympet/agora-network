# RPC (`agora-rpc`)

Access layer for wallets, explorer, and CEX gateways.

## Methods (planned)

| Method | Purpose |
| --- | --- |
| `agora_getDagTips` | Current DAG tips |
| `agora_getBlock` | Block by hash |
| `agora_submitTransaction` | Broadcast a signed tx |
| `agora_getBalance` | Address balance |

Transport (HTTP JSON-RPC / REST) is wired in Phase 5 inside `node-bin`.
