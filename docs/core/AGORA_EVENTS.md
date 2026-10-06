# Agora Events

**Maturity:** Scaffold

## Purpose

Events are community calls, office hours, meetups, hackathons, and the annual summit described in the program note. An event calendar is not a consensus log.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | No event type. Ticket sales, if added later, would be DRC payments plus a community record. Neither ticket object exists. |
| community | Schedules and venues are community-submitted. No calendar store is in this commit. |
| indexer | No event index. |
| private | Personal attendance and contact details would be private. No store exists. |

## Trust boundary

A listed event is not proof that a hub is accredited or that a treasury paid for it. Accreditation is the hub record. Payment is a treasury or DRC transaction. Both are separate.

## Maturity

Scaffold. The calendar is a sentence list in [`docs/community/COMMUNITY_PROGRAMS.md`](../community/COMMUNITY_PROGRAMS.md).

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- The program list of event kinds.

## Planned

- Event records with a source label, optional hub id, and tests. A light-client calendar widget is out of scope until those records exist.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).

