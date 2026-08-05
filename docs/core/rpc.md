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

Transport (HTTP JSON-RPC / REST) is wired inside `node-bin` as the node surface expands.
