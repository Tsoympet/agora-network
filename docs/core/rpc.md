# RPC (`agora-rpc`)

Access layer for wallets, explorer, faucet, and CEX gateways.

## Methods

| Method | Purpose |
| --- | --- |
| `agora_getDagTips` | Current DAG tips (hex hashes) |
| `agora_getBlock` | Block by hash |
| `agora_submitTransaction` | Admit a signed tx into the mempool and gossip it |
| `agora_getBalance` | Address balance (UTXO scan + optional testnet overlay) |
| `agora_fundAddress` | Testnet credit path (disabled on node unless allowed) |
| `agora_getBlockTemplate` | Mining template header (tips as parents) |
| `agora_submitBlock` | Admit a mined block (PoW verify + store + gossip) |

## Dispatch

- `RpcBackend` — trait implemented by node services
- `InMemoryBackend` — ledger + tips/blocks/mempool for tests and the faucet scaffold
- `RpcDispatcher` — parses `RpcRequest`, returns `RpcResponse` with `result` or `error`

## HTTP transport (`agora-node`)

Wired in `core/node-bin`:

| Env | Default | Meaning |
| --- | --- | --- |
| `AGORA_RPC_BIND` | `127.0.0.1:8545` | HTTP JSON-RPC listen address |
| `AGORA_RPC_ALLOW_FUND` | unset | When `1`/`true`, enable `agora_fundAddress` |
| `AGORA_POW_ALGO` | `randomx` | `randomx` or `kheavyhash` for admission / templates |
| `AGORA_TEMPLATE_BITS` | `1` | Initial DAA difficulty (`header.bits`); retargets after admits |

Endpoints:

- `GET /health` → `{"ok":true}`
- `POST /` or `POST /rpc` → JSON body is an `RpcRequest`

Example:

```bash
curl -s http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getDagTips","params":[]}'
```

The live backend (`NodeBackend`) reads tips/blocks/UTXOs from `StateStore`, admits signed transactions via `Mempool`, and publishes them on libp2p gossip.
