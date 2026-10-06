# Agora Reputation

**Maturity:** Scaffold

## Purpose

Reputation is a contribution score derived from evidence. It is separate from TLT ownership, OVL stake, and DRC balances. It is not purchasable and not transferable. On this commit the score itself is a design target.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Passport categories on an attestation can later be evidence. No reputation balance is committed in consensus state. |
| community | A future scoring policy published by hubs or the assembly. Nothing in this tree computes a score. |
| indexer | A future index could aggregate public attestations. No such index exists here. |
| private | Scores must not be derived from private profile fields. Those fields are also absent. |

## Trust boundary

Until a scoring function and its inputs are specified in code, a client must not show a reputation number. A fixture number would cross the trust boundary this slice refuses to cross.

## Maturity

Scaffold. The separation of planes is written in [`docs/community/AGORA_PASSPORT.md`](../community/AGORA_PASSPORT.md). The score plane has no module.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Documentation of the four planes: token ownership, validator stake, contribution reputation, and verified personhood.
- Passport categories that name the kinds of evidence a later score could cite.

## Planned

- A reputation state object, event log, and tests that reject transfer and direct conversion into spendable value.
- A client display that labels each input as blockchain, community, or indexer.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).

