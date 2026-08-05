# RPC (`agora-rpc`)

Access layer for wallets, explorer, and CEX gateways.

Transaction status and confirmations are **acceptance-aware**. Inclusion in a blue block is not sufficient to report a transaction as confirmed.

## Methods

| Method | Purpose |
| --- | --- |
| `agora_getDagTips` | Current DAG tips |
| `agora_getBlock` | Block by hash |
| `agora_submitTransaction` | Broadcast a signed tx |
| `agora_getBalance` | Address balance (accepted UTXOs only, when wired) |
| `agora_getBlockAcceptance` | `AcceptanceBitmap` + fee/reward summary for a block |
| `agora_getTxConfirmation` | `TxConfirmation` from acceptance depth |

## Explorer views

| Type | Role |
| --- | --- |
| `BlockAcceptanceView` | Bitmap + accepted fees + coinbase reward |
| `TxAcceptanceView` | Status + confirmation for a tx id |

`apps/explorer` should consume these views (via generated `agora-types` bindings) rather than inferring finality from GHOSTDAG color.

## Dispatch / transport

- `dispatch(req, &mut dyn RpcBackend)` maps method names → acceptance-aware handlers.
- `agora-node` exposes a **JSON line protocol** on `AGORA_RPC_BIND` (default `127.0.0.1:18545`): one JSON `RpcRequest` per line → one JSON response per line.
