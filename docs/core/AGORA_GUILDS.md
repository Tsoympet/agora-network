# Agora Guilds

**Maturity:** Scaffold

## Purpose

Guilds are cohorts with a charter: builders, node operators, miners, merchants, security reviewers, educators, and local community groups. A guild directory is not a custody wallet.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | No guild account type. Builder funding is described as the OVL Builder treasury when that spend path exists. The spend path is not live. |
| community | Charters and membership lists are community data. `COMMUNITY_PROGRAMS.md` names Agora Builder Guild and Agora Node Guild. |
| indexer | No guild index. Uptime and concentration figures for a node guild would be indexer or node telemetry, and they are not collected here. |
| private | Member spend keys stay in member vaults. A guild record must not contain them. |

## Trust boundary

Membership in a guild does not grant consensus power. OVL and DRC validator sets remain the staking modules. A guild that later holds a multisig uses the same rules as a hub treasury: more than one key, and no silent single-key spend.

## Maturity

Scaffold. Names and intended programs only.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Program text in [`docs/community/COMMUNITY_PROGRAMS.md`](../community/COMMUNITY_PROGRAMS.md).

## Planned

- Charter objects, membership rules, and tests that a guild payload cannot contain a seed or extended private key.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).

