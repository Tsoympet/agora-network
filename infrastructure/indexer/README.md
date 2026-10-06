# Indexer

**Maturity: Scaffold.**

Operator-run HTTP index. `chainProof` is always false. The process does not
invent TLT, OVL, or DRC balances. `AGORA_INDEXER_UPSTREAM_RPC` is the full-node
JSON-RPC URL. When it is unset, `/v1/indexer/status` reports `chainData: null`
and `/v1/indexer/rpc` returns 503.

Devices call this over HTTP through `apps/shared/data-plane`. They do not import
this package. Trust: the operator can omit or delay rows, and an indexed row is
not a light-client proof.

See [`docs/core/AGORA_DATA_PLANE_SPLIT.md`](../../docs/core/AGORA_DATA_PLANE_SPLIT.md).
