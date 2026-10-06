# Agora Missions

**Maturity:** Scaffold

## Purpose

A mission is a sponsored task with a lifecycle and completion evidence. Rewards, when they exist, come from a named protocol treasury under governance rules. Recording a mission is not the same as paying it.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | `MissionRecord` in `community_protocol.rs` and `register_mission_into` in `community_state.rs` are the canonical shape. Commitment happens when a library caller writes state. Block admission is open. |
| community | Discussion of a mission may stay off-chain. The evidence hash is what the record stores. |
| indexer | No mission index beyond `list_missions` on the node. |
| private | Assignee keys are addresses on the record. The assignee’s spend key is not stored. |

## Trust boundary

Allowed transitions are Open to Assigned, then Assigned to Completed with evidence, plus Cancelled. Tests live in the state-machine module. `reward` and `reward_treasury` are fields. Applying the transition does not debit a treasury balance.

## Maturity

Scaffold, matching [`docs/community/GRANTS_AND_MISSIONS.md`](../community/GRANTS_AND_MISSIONS.md) and [`community-registry.md`](community-registry.md).

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- `MissionStatus`: Open, Assigned, Completed, Cancelled.
- Registration and list helpers, plus transition tests.
- Read inclusion through `agora_getCommunityRegistry` when the node has the records.

## Planned

- Signed consensus admission.
- Treasury disbursement on completion, with an asset match to `reward_treasury`.
- A mission screen on the light client. The screen is not in this commit, so none was added.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Prior note: [`docs/community/GRANTS_AND_MISSIONS.md`](../community/GRANTS_AND_MISSIONS.md).

