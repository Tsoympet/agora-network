# Agora Passport

**Maturity:** Scaffold

## Purpose

Passport is a non-transferable contribution attestation. It is not a coin, not stake, and not a claim of verified personhood. The earlier scaffold at [`docs/community/AGORA_PASSPORT.md`](../community/AGORA_PASSPORT.md) still holds the category list. This file adds source labels and the current code boundary.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | A registered `PassportAttestation` would sit in the canonical community registry once a consensus path commits it. Today `register_passport_attestation_into` writes that record only when a caller invokes the state-machine library. |
| community | Issuer policy documents and any future public revocation list that is not yet a consensus object. |
| indexer | No passport indexer is on this commit. `agora_getCommunityRegistry` returns the node’s view of the registry. |
| private | The signed attestation is public by design. Email, language, and device vault material are not fields of `PassportAttestation`. |

## Trust boundary

Verification is a secp256k1 signature over `agora-passport-attestation-v1`, chain id, and genesis hash (`core/crates/crypto/src/passport.rs`). Changing the subject, chain, or genesis fails verification in the crate tests. The registry accepts an attestation only from an active hub coordinator and stores an issuer nonce. A node that has not applied the library call can omit attestations from the read RPC. Clients must label that read as node-reported.

## Maturity

Scaffold. The signed envelope and registry checks exist. Revocation, block admission, and a wallet screen do not, so the module stays at the same maturity as [`community-registry.md`](community-registry.md).

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- Types in `core/crates/types/src/passport.rs` and signing in `core/crates/crypto/src/passport.rs`.
- Registry rules and tests in `core/crates/state-machine/src/community_state.rs`.
- Read path `agora_getCommunityRegistry` includes passport records the node has stored.
- No transfer API is exposed.

## Planned

- Public revocation and a tested replay rule for revoked ids.
- Consensus block admission.
- A light-client passport screen that shows the signature status separately from node-reported registry rows.
- Sybil resistance beyond the issuer policy. The existing community note already refuses that claim, and this file does the same.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Prior note: [`docs/community/AGORA_PASSPORT.md`](../community/AGORA_PASSPORT.md).
- Registry: [`community-registry.md`](community-registry.md).

