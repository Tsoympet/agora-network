# Agora Passport

**Maturity:** Scaffold.

Privacy-conscious, **non-transferable** contribution and reputation system. Start with signed attestations and transparent issuer policies. **Do not claim complete Sybil resistance** unless implemented.

## Separated planes

- Token ownership
- Validator stake
- Contribution reputation
- Verified personhood

Reputation is not purchasable, transferable, or directly convertible into unrestricted financial value.

## Attestation categories

Code, documentation, translation, security reports, running infrastructure, hosting events, teaching, merchant onboarding, grant reviewing, governance participation, moderation, community support.

## Verification

Issuers publish policies; attestations are secp256k1-signed and domain-separated (`agora-passport-attestation-v1`). Revocation lists are public. Missions completion may mint attestations (see grants doc).
