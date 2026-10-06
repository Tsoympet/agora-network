# Agora Treasury

**Maturity:** Scaffold

## Purpose

Three protocol treasuries keep the asset roles apart: TLT Security, OVL Builder, and DRC Community. Community grants point at one of them. Pointing is not spending.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | `TreasuryId` and `TreasuryBalance` in `core/crates/types/src/treasury.rs`. Balances are initialized in `core/crates/state-machine/src/governance_state.rs`. |
| community | `ProposalKind::TreasurySpend` in the civic engine is an administrative proposal. Forum worker ideas can precede it (`docs/governance/COMMUNITY.md`). |
| indexer | No public financial report index. A node that returns treasury balances is serving its state, not an independent report. |
| private | Tamias keys, when they exist, stay in validator or officer vaults. They are not treasury document fields. |

## Trust boundary

Each `TreasuryBalance` is bound to one native asset. Mixing an OVL treasury with a DRC amount fails the type constructor in tests. Grant registration does not call the treasury debit path. Civic `execute` for `TreasurySpend` updates proposal status in the governance engine. [`governance.md`](governance.md) states that most executed proposals have no protocol side effects.

## Maturity

Scaffold for community disbursement. The balance types exist and are covered by unit tests. The spend path into canonical balances is open, so the community treasury module stays Scaffold.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- `TreasuryId::TltSecurity`, `OvlBuilder`, `DrcCommunity` and asset-matched balances.
- Genesis initialization and grant records that name a treasury.
- Civic sponsorship checks: a treasury proposal needs a Tamias sponsor before voting opens, in the engine tests.

## Planned

- Consensus execution that debits the named treasury, credits the beneficiary in the same asset, and is covered by state tests.
- Public reports generated from that execution log.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Types: `core/crates/types/src/treasury.rs`.
- Grants: [`AGORA_GRANTS.md`](AGORA_GRANTS.md).

