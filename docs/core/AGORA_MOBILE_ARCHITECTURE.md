# Agora mobile architecture

**Maturity:** Single-node prototype

## Purpose

The phone app is an Expo client in `apps/mobile`. It is a light client. It stores a vault, signs on device, and talks to a full node over JSON-RPC. It does not sync the BlockDAG as a node and does not run RandomX.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | The phone uses the shared verifier for the header spine and TLT inclusion. Balances and DRC objects are RPC reads. |
| community | No community program screens. Civic ballots are on the desktop and explorer shells, not a dedicated phone assembly. |
| indexer | None on device. |
| private | The mnemonic is sealed with AES-256-GCM and stored through Expo SecureStore. SecureStore is the phone vault adapter documented in `agora-light-client.md`. |

## Trust boundary

The phone trusts the configured RPC for anything the shared verifier does not recompute. `EXPO_PUBLIC_AGORA_RPC_URL` must be reachable from the device. A localhost URL on a laptop is not a phone endpoint. The bearer token, when set, stays in SecureStore and out of pairing payloads.

## Maturity

Single-node prototype, same as the shared light client.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- `apps/mobile/App.tsx` with vault, send, pairing, light-client status, and capability panel.
- Shared code under `apps/shared/light-client`.
- Watch-only import that waits for a matching network and genesis.

## Planned

- Background sync as a node, local PoW, and community program screens.
- MY AGORA beginner home. Documented as PLANNED in the architecture note. Not added to this shell.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Light client: [`AGORA_LIGHT_CLIENT.md`](AGORA_LIGHT_CLIENT.md).
- PC: [`AGORA_PC_ARCHITECTURE.md`](AGORA_PC_ARCHITECTURE.md).

