# Agora privacy model

**Maturity:** Scaffold

## Purpose

Privacy for community features means keeping keys and personal profile fields off the chain and off public indexes, while keeping contribution attestations publicly checkable. The model is a target plus the constraints the current types already have.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Passport attestations, hub records, grants, and missions are public once committed. Their schemas do not include email or language. |
| community | Forum posts are public administrative records. A future community API must not echo private profile fields on unauthenticated routes. |
| indexer | An indexer must not promote a private field into a public document. No indexer is on this commit, so this rule is a requirement for C21. |
| private | Mnemonic, vault password, RPC bearer token, and future email and language. Device vaults exist for the mnemonic. Email and language have no store. |

## Trust boundary

Public verifiability of a passport signature is intentional. Hiding the subject would break the attestation. The boundary is the field set: evidence and subject address are public. Contact details are not part of the struct. Pairing makes the same split: watch-only is public account data. Restore is an explicit copy of the spend phrase between two devices the person controls.

## Maturity

Scaffold. Vault behavior is implemented inside the light client and is specified in the light-client docs. A profile privacy design with tests is still a scaffold.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Attestation and hub structs without contact fields.
- Device vaults and pairing rules that keep the mnemonic off RPC and off watch payloads.
- Domain-separated passport signatures so an attestation does not verify on another chain id or genesis.

## Planned

- An authenticated private profile, retention rules, and tests that consensus encodings and public community responses omit those fields.
- A revocation design that does not require publishing the underlying evidence document.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Passport: [`AGORA_PASSPORT.md`](AGORA_PASSPORT.md).
- Light client: [`AGORA_LIGHT_CLIENT_SECURITY_MODEL.md`](AGORA_LIGHT_CLIENT_SECURITY_MODEL.md).

