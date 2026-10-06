# Agora PC architecture

**Maturity:** Single-node prototype

## Purpose

The PC app is the Vite and Tauri shell in `apps/desktop`. It is the same light client as the phone, with a browser vault and a civic ballot panel. It is not a full node.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Shared verifier for headers and TLT inclusion. Native balances and submissions go through JSON-RPC. |
| community | `GovernancePanel` posts forum signals and ballots to the node’s administrative governance RPC. |
| indexer | Tip lists and transaction watches are RPC polls, not an indexer. |
| private | The sealed vault blob is in `localStorage`. The RPC token is stored separately and is excluded from pairing payloads. |

## Trust boundary

Desktop and phone must not be treated as two authorities. After pairing, each talks to a full node on its own. The PC can be off. A watch-only phone cannot sign. A restore code is the spend key.

## Maturity

Single-node prototype.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- `apps/desktop/src/App.tsx` and panels for pairing, light-client status, capabilities, and governance.
- `localStorage` vault and the shared verifier.
- Dev entry `npm run dev` against `VITE_AGORA_RPC_URL`.

## Planned

- Embedding `agora-node` inside the wallet.
- Beginner home widgets. The Docs / About panel added with this slice is a document index, not that home.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Light client: [`AGORA_LIGHT_CLIENT.md`](AGORA_LIGHT_CLIENT.md).
- Phone: [`AGORA_MOBILE_ARCHITECTURE.md`](AGORA_MOBILE_ARCHITECTURE.md).

