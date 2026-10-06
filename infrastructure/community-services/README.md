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

`AGORA_COMMUNITY_STORE` is the JSON file for the catalog, forum replies, and
mission reviews. When the process is started directly and the variable is
unset, the file is `data/community-store.json` beside this server. That file
is infrastructure trust: the operator can edit it. It is not consensus and it
does not hold treasury balances or seeds. Short-lived session tokens stay in
process memory.

Responses are envelopes: `plane: "infrastructure"`, `chainProof: false`, and
`data`. Device apps reach this process only through
`apps/shared/data-plane`. There is no `/treasury` route.

Trust for each mounted service is in
[`docs/core/AGORA_DATA_PLANE_SPLIT.md`](../../docs/core/AGORA_DATA_PLANE_SPLIT.md).
