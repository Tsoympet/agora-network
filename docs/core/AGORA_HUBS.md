# Agora Hubs

**Maturity:** Scaffold

## Purpose

Hubs are geographic or specialist communities. No hub owns an exclusive territory. This note matches [`docs/community/AGORA_HUBS.md`](../community/AGORA_HUBS.md) and states what the registry code actually checks.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | `HubRecord` is the canonical shape: id, name, classification, charter hash, coordinators, multisig address, election and reporting periods, COI root, deliverables root, accreditation proposal id, and status. |
| community | Charter text itself can live outside the chain. The registry stores the charter hash. |
| indexer | No hub directory index and no map. |
| private | Coordinator spend keys are not hub fields. The multisig address is public. |

## Trust boundary

Active status requires the checks implemented in `register_hub_into`, including a non-zero multisig and an accreditation proposal id. Active coordinators enter the passport issuer index. Signed accreditation and revocation transactions are not a block lane yet, so a hub row is only as authoritative as the state the node built with the library API.

## Maturity

Scaffold.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- `HubKind` Geographic and Specialist. Status Pending, Active, Suspended, Revoked.
- Registry validation, issuer index, list API, and tests in `community_state.rs`.

## Planned

- Consensus operations for accredit and revoke.
- A hub directory screen. Not added here.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Prior note: [`docs/community/AGORA_HUBS.md`](../community/AGORA_HUBS.md).

