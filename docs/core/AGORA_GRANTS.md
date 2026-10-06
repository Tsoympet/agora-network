# Agora Grants

**Maturity:** Scaffold

## Purpose

Grants fund public work from one protocol treasury, with ordered milestones and public evidence. DRC Community grants also require a conflict-of-interest disclosure. The registry records eligibility. It does not send the funds.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | `GrantRecord` binds proposal id, `TreasuryId`, beneficiary, total, and milestones. `register_grant_into` writes the community registry when called. |
| community | Review discussion can stay off-chain. The disclosure root and deliverable hashes are what the record keeps. |
| indexer | No grant indexer. Node list RPC is the read path. |
| private | Reviewer notes that are not hashed into the record have no store here. Beneficiary spend keys are not grant fields. |

## Trust boundary

Milestone acceptance requires the next exact deliverable hash and cannot exceed the cap (`community_protocol.rs`). DRC Community grants without a cleared, non-zero conflict-of-interest disclosure fail registration. The `released` amount is a counter on the grant object. Treasury balances live under a different key and are not updated by that counter.

## Maturity

Scaffold, same as the registry note.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Grant schema, milestone transition checks, COI gate, registry tests, and list API.

## Planned

- Signed disbursement that debits the bound treasury in the same asset and fails closed on a mismatch.
- A grants screen. Not added in this slice.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Prior note: [`docs/community/GRANTS_AND_MISSIONS.md`](../community/GRANTS_AND_MISSIONS.md).
- Treasuries: [`AGORA_TREASURY.md`](AGORA_TREASURY.md).

