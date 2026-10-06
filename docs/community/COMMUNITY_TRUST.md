# Community trust (sections 42–47)

**Maturity:** Scaffold.

This document covers local communities, proposal transparency, configurable
rewards, public analytics, and portable identity. The light-client assumptions
live in [`docs/core/LIGHT_CLIENT_SECURITY_MODEL.md`](../core/LIGHT_CLIENT_SECURITY_MODEL.md).

Sections 1–41 stay in the community light-client slice this branch stacks on.
Hub, passport, grant, mission, and forum screens are unchanged. This module
reads those hub, proposal, and ecosystem records and adds the trust fields
they do not carry. A proposal marked on-chain without a commitment id keeps
the ADVISORY badge.

The rules are implemented in `agora_governance::community_trust` and mirrored
in `apps/shared/community-trust` for the desktop and phone screens. The phone
and PC keep the same labels. Neither screen calls GPS, and neither screen
sends a mnemonic to a community API.

## Local communities

Attributes are free text: country, region, city, language, interest, and
specialization. There is no country enum. Greece, Europe, Asia, and the
Americas ship as seed records in `seed_local_communities`.

Search takes the string the user types. An empty query lists the seeds.
Coordinate fields such as `latitude` are rejected. `community_search_uses_device_location`
is false.

## Proposal transparency

Every proposal view has a creator, why, what changes, expected cost, treasury
impact, voting period, eligibility, current vote counts, final result,
implementation status, and commit or release links.

The badge is derived:

| Authority | Badge | Final result |
| --- | --- | --- |
| `on_chain` | `ON-CHAIN` | pending, passed, failed, or expired |
| `advisory` | `ADVISORY` | pending or advisory_recorded |

An advisory record cannot be stored as a binding pass. `shipped` requires a
commit or release link. The link is evidence the publisher attached. It is
not a consensus proof. Example cards on the wallet are scaffold data, not a
live tally from a node.

## Rewards

Programs can offer DRC, OVL, TLT, badges, reputation, certificates, grant
eligibility, program access, and event credentials.

| Kind | Payable path |
| --- | --- |
| Badge, reputation, certificate, grant eligibility, program access, event credential | Attestation or program rule. No coin moves. |
| DRC, OVL | Transfer of already-issued coins when the configured treasury balance covers the amount. |
| TLT from emission | Blocked in the UI. `changes_tlt_emission` is false. `supply_delta_base_units` is 0. |
| TLT from an existing treasury | Payable only as a transfer of already-issued TLT. The PoW schedule is untouched. The button does not submit a transaction. |

This module does not edit `AssetMonetaryPolicy`, the halving interval, or the
TLT maximum supply. Emission is not used to fill a short treasury.

The reward control reports `submits_transaction: false`. A later signed
treasury spend would be a separate consensus path. This scaffold does not
broadcast one.

## Public analytics

The public object is fifteen counters: active users, active developers,
active merchants, missions, grants, bounties, academy, events, governance,
DRC activity, OVL contracts, TLT activity, nodes, miners, and projects.

Unknown keys and private keys (`email`, `address`, `device_id`, and the rest
of the reject list) fail the parser. Arrays of accounts fail because each
counter is an integer. The scaffold report is all zeros and
`private_user_analytics: false` until a public aggregator exists.

## Portable identity

`export_portable_identity` writes `agora-portable-identity-v1` JSON:

- public passport (`agora-public-passport-v1`)
- local private preferences (`agora-local-prefs-v1`): language, typed region query, interest filters
- service trust labels for all ten light-client assumptions

`contains_private_keys` is false. A field named `mnemonic`, `private_key`, or
`xprv` is rejected. A region query of twelve or more words is refused so a
pasted seed phrase does not enter the file. Issuer signatures are labeled
`hub_signed_unchecked` because this export does not re-verify them.

OpenAPI components: [`portable-identity.openapi.yaml`](portable-identity.openapi.yaml).

## Consensus boundary

`community_trust` is a library. It has no block lane, no unsigned mutation
RPC, and no mint function. Registration of hubs, grants, and missions stays
in the existing community registry. Forum posts stay on the community board.
