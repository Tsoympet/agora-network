# Agora community API

**Maturity:** Scaffold

## Purpose

The community API is a future HTTP surface for public community records and authenticated private profile reads. It is not the node JSON-RPC, and it is not a consensus writer.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | When built, public routes may mirror registry reads from a node. They must keep the node-reported label. This commit has no such proxy. |
| community | The service itself would be the community source for directories, forum caches, and profiles. It is not in the tree. |
| indexer | The API might sit in front of an indexer. C21 is also open, so there is nothing to front. |
| private | A session, if added, must be a wallet signature over a challenge. The body must not be a mnemonic. Tokens must expire. Private fields stay off public routes. |

## Trust boundary

Node RPC methods such as `agora_getCommunityRegistry` and the forum methods are the node’s own surface, documented in [`rpc.md`](rpc.md) and [`governance.md`](governance.md). Calling those methods from a wallet is not a community API. A community API that accepted unsigned mutations or seeds would cross the boundary this design forbids. No server in this commit implements it.

## Maturity

Scaffold. The interface is specified here so later work has a boundary. Absence of the server is why C20 is PLANNED.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Node JSON-RPC for registry reads and administrative forum posts, which remain node surfaces.
- This boundary statement.

## Planned

- A small HTTP process with public GET routes, a signed session for private fields, rate limits, and tests. Light clients may call it later. They still must not embed a full node.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Node RPC: [`rpc.md`](rpc.md).

