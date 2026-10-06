# Agora community architecture

**Maturity:** Scaffold

## Purpose

This note is the map for Agora community surfaces on Trident L1. It records where each record is allowed to live, which client may display it, and the target information architecture for MY AGORA. Wallet and explorer screens on this base stay readers and signers. They do not become full nodes.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Canonical L1 state: TLT UTXOs, OVL accounts and execution, DRC accounts, treasury balances, and the community registry root under `community/v1/*` when a state-machine call has committed it. |
| community | Off-consensus community records. The civic forum in `agora-governance` is local node Meta-CF state (`meta/governance`), which [`community-registry.md`](community-registry.md) excludes from the canonical community root. |
| indexer | A derived view of chain or community data. The explorer reads node RPC. A community indexer with source labels is PLANNED. |
| private | Keys, mnemonics, vault passwords, and future profile fields such as email and language. These stay on the device or in a future authenticated store. They are outside consensus. |

## Trust boundary

A light client trusts one full node for RandomX work, GHOSTDAG parent selection, and validator signatures. It recomputes the checks listed in [`agora-light-client.md`](agora-light-client.md). Community registry reads from `agora_getCommunityRegistry` are node-reported. Registration functions in `core/crates/state-machine/src/community_state.rs` are library calls. This base has no community block lane and no unsigned mutation RPC for hubs, passports, grants, or missions.

Civic ballots in `core/crates/governance` are an administrative prototype. [`governance.md`](governance.md) states that RPC callers supply voter address and balances, votes are unsigned, and most executed proposals have no protocol side effects.

## Maturity

Scaffold for the community product. Consensus code for the three assets is further along than the community product and is still short of a public network. This document uses Scaffold for the community map. It does not raise any community module above Scaffold, Experimental, or Single-node prototype.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Remote branches `cursor/agora-community-architecture-cdcf`, `cursor/agora-community-trust-cdcf`, and `cursor/agora-community-ecosystem-cdcf` were absent. Uncommitted files in other worktrees are outside this base.

## Implemented

- Three assets stay separate in the state machine and in `apps/shared/light-client` (TLT UTXO, OVL account, DRC account).
- Canonical community schemas and a rolling root exist for hubs, passport attestations, grants, and missions, with read RPC `agora_getCommunityRegistry`.
- A civic engine, forum RPC (`agora_listForumTopics`, `agora_postForumTopic`, `agora_ackConstitution`), and ballot panels exist on desktop and explorer. That state is administrative.
- PC (`apps/desktop`) and phone (`apps/mobile`) light clients share `apps/shared/light-client`, including pairing that can omit the mnemonic on the watch-only path.

## Planned

- Consensus admission for community registrations, signed ballots, treasury disbursement, reputation, academy, bounties, guilds, merchant directory, events, community API, and a labeled indexer.
- MY AGORA beginner home, described below. It is absent from `apps/desktop`, `apps/mobile`, and `apps/explorer` on this base.

## Final product vision

MY AGORA is the gateway a person opens first. It is an orientation surface over the same light client, with documents and chain reads labeled by source. It is a full node only for operators who run `agora-node`.

The four-role principle, plus the network that connects them:

| Role | Asset or layer | What a person should understand |
| --- | --- | --- |
| DRC moves | DRC | Payments, invoices, and merchant settlement. DRC is never mined. |
| OVL builds | OVL | Execution and builder funding. OVL is never mined. |
| TLT secures | TLT | PoW work that orders the BlockDAG. TLT is the only mineable asset. |
| Community participates | Community records | Hubs, passports, missions, assembly, grants. Participation is not a fourth coin. |
| Network connects | Agora Network | One L1, light clients, and future community services. Clients follow the network. They do not each carry the chain. |

Finality stays the existing rule: a TLT work threshold and independent two-thirds quorums of OVL stake and DRC stake. Community participation does not mix those stakes and does not add a price oracle.

## Beginner home

**Status: PLANNED.**

No beginner home is mounted in the desktop wallet, the phone wallet, or the explorer. This slice does not add dashboard widgets, live balances, or role cards that pretend those programs are open.

Target information architecture, when a later change adds the home:

1. One sentence of the four-role principle, marked as orientation copy.
2. Three balance rows only after the existing light-client read succeeds, each labeled node-reported unless a local proof exists.
3. Links into Passport, Missions, Assembly, and DRC pay, each hidden or marked PLANNED until that module has a real screen.
4. The Docs / About index that this slice adds, which links these markdown files and does not query the chain.
5. Empty and error states that repeat the source label. A missing RPC is an error, not a zero balance presented as verified.

## Related

- Earlier notes, left unchanged: [`docs/community/COMMUNITY_PROGRAMS.md`](../community/COMMUNITY_PROGRAMS.md), [`docs/core/community-registry.md`](community-registry.md), [`docs/governance/COMMUNITY.md`](../governance/COMMUNITY.md).
- Phase table: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).

- Light client behavior: [`agora-light-client.md`](agora-light-client.md).
- Asset rules: [`docs/assets/NATIVE_ASSETS.md`](../assets/NATIVE_ASSETS.md).
