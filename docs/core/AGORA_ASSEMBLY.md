# Agora Assembly

**Maturity:** Experimental

## Purpose

Assembly is the civic ballot and the public square: constitution, chambers, proposals, and forum topics. Binding governance is the target. The running engine is an administrative prototype.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | The intended home for signed proposals, deposits, and tallies. This commit does not admit those acts in a block. |
| community | `CommunityBoard` forum topics and constitution acknowledgements persist with governance in node Meta-CF (`meta/governance`). [`community-registry.md`](community-registry.md) excludes that board from the canonical community root. |
| indexer | Explorer and desktop panels render the node RPC view. They are not a separate indexer. |
| private | Voter keys should sign ballots. Today the RPC accepts a caller-supplied address. The vote is not a private ballot. It is an unsigned local record. |

## Trust boundary

From [`governance.md`](governance.md): callers supply voter address, claimed balances, and slots. Deposits are not locked UTXOs. Different nodes can diverge. A light client that shows a tally is showing that node’s administrative state. Quorum math for OVL and DRC finality is a different subsystem and is not this assembly.

## Maturity

Experimental. The crate, JSON-RPC methods, desktop `GovernancePanel`, and explorer ballot section exist and have engine tests. [`governance.md`](governance.md) calls the wiring an administrative prototype. Experimental is the matching label in the allowed set. It is not a public network.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- `core/crates/governance`: constitution, ranks, chambers, proposal lifecycle, forum topics.
- RPC: `agora_listForumTopics`, `agora_postForumTopic`, `agora_ackConstitution`, plus the governance read and vote methods used by the panels.
- Engine tests for timelock, treasury sponsorship checks, and constitution assent. Those tests do not move canonical treasury balances.

## Planned

- Signed vote payloads, consensus balance snapshots, locked deposits, and deterministic execution with protocol side effects.
- A clear client label that today’s tally is administrative community state.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Civic overview: [`docs/governance/CIVIC_MODEL.md`](../governance/CIVIC_MODEL.md).
- Engine note: [`governance.md`](governance.md).
- Forum note: [`docs/governance/COMMUNITY.md`](../governance/COMMUNITY.md).

