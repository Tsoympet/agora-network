# Agora light client security model

**Maturity:** Experimental

## Purpose

This file is the security boundary for the PC and phone light client. `docs/core/LIGHT_CLIENT_SECURITY_MODEL.md` is not in the tree. The working notes are the Threat model section of [`agora-light-client.md`](agora-light-client.md) and the node-wide [`docs/security/THREAT_MODEL.md`](../security/THREAT_MODEL.md). This page links those and states which checks the device performs.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Local checks cover header hash, parent linkage, blue-score direction along the supplied spine, genesis, TLT Merkle inclusion, body-root folds, and two-thirds quorum arithmetic on supplied stake totals. |
| community | Community and civic RPC responses are not included in header proofs. They are outside this verification set. |
| indexer | No second source of truth. Comparing two nodes is future work named in the companion doc. |
| private | Signing keys stay on device. An RPC response is not a key. Pairing watch payloads exclude the mnemonic. Restore payloads are the spend secret and are documented as such. |

## Trust boundary

An RPC operator can hide transactions, offer a self-consistent spine that is not the network’s, and report stake totals that do not match validator signatures. The device rejects mismatched Merkle siblings, dropped body-binding layers, genesis mismatches, and missing quorum fields. It does not re-execute RandomX and does not verify validator signatures. `pow_checked_by` remains `full_node`. Balances that claim `header_proven` are rejected.

## Maturity

Experimental. The checks above are implemented and tested in the light-client package, and the residual trust in one node is still the dominating assumption. Experimental matches a partial model. The wallet maturity remains Single-node prototype in [`AGORA_LIGHT_CLIENT.md`](AGORA_LIGHT_CLIENT.md).

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- The checks and the explicit trust list in `agora-light-client.md`.
- Fail-closed watch-only signing (`watch-only wallet cannot sign or spend`).
- RPC URL checks: `http` and `https` only, embedded userinfo rejected, loopback warned and omitted from watch pairing.

## Planned

- Local PoW and signature verification, or a multi-node quorum for the spine.
- A standalone `LIGHT_CLIENT_SECURITY_MODEL.md` can replace this pointer later. Until then this file is the alias.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Companion threat section: [`agora-light-client.md`](agora-light-client.md).
- Node threat model: [`docs/security/THREAT_MODEL.md`](../security/THREAT_MODEL.md).

