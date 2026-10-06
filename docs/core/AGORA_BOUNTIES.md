# Agora Bounties

**Maturity:** Scaffold

## Purpose

A bounty is a published task that a contributor claims and a reviewer accepts. It is the open-task form of grants and missions. This commit names it and does not implement it.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | No bounty type. A later design can reuse mission or grant records. That reuse is not in code. |
| community | Task text would be community-submitted until a hash is committed. No submission store exists. |
| indexer | No bounty index. |
| private | Claimant keys must not be stored on a bounty object. There is no object yet. |

## Trust boundary

Paying a bounty is a treasury act and has to pass the same asset and authorization rules as grants. A client must not show a bounty as paid because a document listed it.

## Maturity

Scaffold. See the bounties paragraph in [`docs/community/GRANTS_AND_MISSIONS.md`](../community/GRANTS_AND_MISSIONS.md).

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- The name and the funding sketch in the grants note.

## Planned

- State, claim rules, public review, and tests, including a test that claim does not move coins by itself.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).

