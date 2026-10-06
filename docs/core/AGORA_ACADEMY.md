# Agora Academy

**Maturity:** Scaffold

## Purpose

Agora Academy is the education track: courses for developers, miners, validators, merchants, and governance participants, in Greek and English. It is a community program, not a consensus object, until a later design says otherwise.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | No course, lesson, or certificate type is committed on this base. A future completion attestation could be a passport category. That link is not coded. |
| community | Course text and university partnerships are community content. No catalog service is in this commit. |
| indexer | No academy index. |
| private | Lesson progress would be device-local or an authenticated profile. No progress store exists. |

## Trust boundary

Publishing a course must not require a full node, and completing a lesson must not be presented as an on-chain fact until an attestation or other signed object exists. This commit has neither.

## Maturity

Scaffold. The program is named in [`docs/community/COMMUNITY_PROGRAMS.md`](../community/COMMUNITY_PROGRAMS.md) and has no module.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- The program description in `COMMUNITY_PROGRAMS.md`.

## Planned

- A catalog with source labels, local progress, and tests that progress bytes are absent from consensus encodings.
- Optional passport attestations for teaching, issued through the passport rules.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Program list: [`docs/community/COMMUNITY_PROGRAMS.md`](../community/COMMUNITY_PROGRAMS.md).

