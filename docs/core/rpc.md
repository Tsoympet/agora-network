# RPC (`agora-rpc`)

Access layer for wallets, explorer, faucet, and CEX gateways.

## Methods

| Method | Purpose |
| --- | --- |
| `agora_getDagTips` | Current DAG tips (hex hashes) |
| `agora_getBlock` | Block by hash |
| `agora_getTransaction` | Lookup by `tx_id`: `pending` (mempool) / `confirmed` (indexed) / `unknown` |
| `agora_getMempool` | Pending pool snapshot (`count` + fee-ordered `transactions`, optional `limit`) |
| `agora_submitTransaction` | UTXO-check + admit a signed tx into the mempool and gossip it |
| `agora_getBalance` | Address balance (sum of live `cf_utxo`) |
| `agora_getUtxos` | Spendable outpoints for an address (`tx_id`, `index`, `value`) |
| `agora_fundAddress` | Testnet mint: write a spendable `cf_utxo` (disabled unless `AGORA_RPC_ALLOW_FUND`) |
| `agora_getBlockTemplate` | Mining template block (tips as parents + coinbase) |
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
| `AGORA_RPC_ALLOW_FUND` | unset | When `1`/`true`, enable `agora_fundAddress` (mints spendable UTXOs) |
| `AGORA_POW_ALGO` | `randomx` | `randomx` or `kheavyhash` for admission / templates |
| `AGORA_TEMPLATE_BITS` | `1` | Initial DAA difficulty (`header.bits`); retargets after admits |
| `AGORA_MINER_ADDRESS` | `00…00` | Coinbase payout address (40-char hex) for templates |
| `AGORA_PREMINE_ADDRESS` | `00…00` | Genesis premine payout (40-char hex); only applied on a fresh `AGORA_DATA` |
| `AGORA_MIN_RELAY_FEE` | `1` | Minimum implicit fee (`in − out`) for mempool admission |

Endpoints:

- `GET /health` → `{"ok":true}`
- `POST /` or `POST /rpc` → JSON body is an `RpcRequest`
- CORS enabled (`Access-Control-Allow-Origin: *`) for browser explorers; `OPTIONS` preflight supported

`agora_getBlock` returns explorer-friendly JSON (`id`, hex parent hashes, `tx_count`, and hex `transactions` with inputs/outputs).  
`agora_getTransaction` returns `{ tx_id, status, block_id, index, fee, transaction }` — wallets should poll until `confirmed` (missing txs return `status: "unknown"`, not an RPC error). Confirmed locations are indexed in `cf_warm` (`tx/` ‖ tx_id → block_id ‖ index) on admit / genesis.  
`agora_getMempool` returns `{ count, transactions: [{ tx_id, fee, transaction }] }` ordered by fee desc then `tx_id` (default `limit` 128, max 10000).  

`agora_getBlockTemplate` returns a full `Block` (native serde hashes as byte arrays) with a coinbase paying `AGORA_MINER_ADDRESS` for **emission + Σ transfer fees** at the estimated next blue score, followed by up to 128 mempool transfers (fee-desc, then `tx_id`); `header.tx_root` commits to that body. `agora_submitBlock` rejects `tx_root` mismatches and evicts included/conflicting mempool txs. Mempool admission requires `fee ≥ AGORA_MIN_RELAY_FEE`; fees are paid to the miner via the coinbase (not burned).

Example:

```bash
curl -s http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getDagTips","params":[]}'
```

The live backend (`NodeBackend`) reads tips/blocks/UTXOs from `StateStore`, admits signed transactions via `Mempool`, and publishes them on libp2p gossip.

## Light clients

`apps/shared/light-client` provides `createLightClient` + `startTipSync` / `watchTransaction` plus wallet helpers (`getBalance`, `getUtxos`, `submitTransaction`, BIP-39 `sendTransfer`) used by:

- `apps/explorer` (live DAG + tx lookup + mempool panel + pending watch)
- `apps/desktop` (tip sync, UTXO lookup, signed send + confirmation poll)
- `apps/mobile` (tip sync, UTXO lookup, signed send + confirmation poll)

Default endpoint: `http://127.0.0.1:8545/rpc` (explorer/desktop may proxy `/rpc`).
