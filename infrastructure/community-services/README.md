# Community infrastructure host

**Maturity: Experimental.**

One local HTTP process that mounts the indexer, forum server, grant and mission
admin, event service, search, moderation, and notification dispatcher. It is
not a consensus RPC. It does not store seeds, private keys, or treasury
balances.

```bash
cd apps/shared && npm install
node --experimental-strip-types ../../infrastructure/community-services/server.ts
```

`AGORA_COMMUNITY_PORT` defaults to `8787`. `AGORA_INDEXER_UPSTREAM_RPC` is the
optional full-node URL. Without it the indexer returns no chain rows.

Responses are envelopes: `plane: "infrastructure"`, `chainProof: false`, and
`data`. Device apps reach this process only through
`apps/shared/data-plane`. There is no `/treasury` route.

Trust for each mounted service is in
[`docs/core/AGORA_DATA_PLANE_SPLIT.md`](../../docs/core/AGORA_DATA_PLANE_SPLIT.md).
