# RPC (`agora-rpc`)

Access layer for wallets, explorer, faucet, and CEX gateways.

## Methods

| Method | Purpose |
| --- | --- |
| `agora_getDagTips` | Current DAG tips (hex hashes) |
| `agora_getBlock` | Block by hash |
| `agora_submitTransaction` | Admit a signed/unsigned tx into the mempool |
| `agora_getBalance` | Address balance (base units) |
| `agora_fundAddress` | Testnet credit path (faucet / local backend) |

## Dispatch

- `RpcBackend` — trait implemented by node services
- `InMemoryBackend` — ledger + tips/blocks/mempool for tests and the faucet scaffold
- `RpcDispatcher` — parses `RpcRequest`, returns `RpcResponse` with `result` or `error`

HTTP / REST transport remains wired from `node-bin` (Phase 5). The faucet and other infra tools can call the dispatcher in-process today.
