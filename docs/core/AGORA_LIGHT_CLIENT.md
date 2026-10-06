# Agora light client

**Maturity:** Single-node prototype

## Purpose

This is the index for the Agora Network light client on PC and phone. Operational detail, RPC names, and the threat notes stay in [`agora-light-client.md`](agora-light-client.md). The two files describe the same prototype.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Header hashes, selected-parent links, genesis binding, TLT Merkle inclusion, and body-root binding are recomputed from node-supplied bytes. OVL and DRC balances and ledger objects are node reads. |
| community | The light client can call civic and community-registry RPC. Those responses are the node’s state, labeled node-reported. |
| indexer | The client does not run an indexer. History and mempool views come from RPC. |
| private | The BIP-39 phrase and vault key live in `localStorage` on desktop and Expo SecureStore on the phone. The node does not receive them. |

## Trust boundary

One full node is the source of PoW checks, GHOSTDAG blue sets, and validator signatures. The client rejects a payload that sets `header_proven` on a balance, and it fails closed when a header window does not reach genesis. Detail is in the companion note and in [`AGORA_LIGHT_CLIENT_SECURITY_MODEL.md`](AGORA_LIGHT_CLIENT_SECURITY_MODEL.md).

## Maturity

Single-node prototype, the same label as [`agora-light-client.md`](agora-light-client.md). PRs #152 and #153 are the wallet and capability surfaces this index points at.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Shared verifier `apps/shared/light-client`, desktop shell, phone shell, pairing, and feature matrix with verified-locally, node-reported, and unavailable labels.
- Tests in that package, including feature matrix, pairing, and TLT inclusion.

## Planned

- Multi-node comparison, local RandomX, local validator signature checks, and header proofs for OVL and DRC state.
- Covenant spends and an OVL EVM deploy UI, which the feature matrix already marks unavailable.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Companion: [`agora-light-client.md`](agora-light-client.md).
- Security: [`AGORA_LIGHT_CLIENT_SECURITY_MODEL.md`](AGORA_LIGHT_CLIENT_SECURITY_MODEL.md).

