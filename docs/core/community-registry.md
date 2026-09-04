# Canonical community registry

**Maturity:** Scaffold.

Phase 5b defines canonical schemas and a bounded state commitment for Agora
Hubs, Passport attestations, Grants, and Missions. The registry is initialized
at genesis and exposed through `agora_getCommunityRegistry`.

## Hubs

Canonical Hub records include a stable ID, public name, geographic/specialist
classification, charter hash, unique coordinators, accreditation proposal, and
status. Active Hubs require a canonical accreditation proposal, non-zero
multisig address, election/reporting periods, and COI commitment.

## Passport

Passport attestations are secp256k1-signed under
`agora-passport-attestation-v1`. The signature binds issuer, subject, category,
evidence, issuer policy, epochs, nonce, chain ID, and genesis hash. Changing the
subject invalidates the signature; there is no transfer API.

Only coordinators indexed from active canonical Hubs may issue attestations.
Issuer nonces prevent replay. This is contribution evidence, not a claim of
complete Sybil resistance or verified personhood.

## Grants and Missions

Grant schemas bind a governance proposal, one asset-fixed protocol treasury,
beneficiary, total, and ordered milestones. Milestone acceptance requires the
next exact deliverable hash and cannot exceed the grant cap. Mission schemas
enforce Open → Assigned → Completed transitions with completion evidence.
DRC Community grants cannot enter the canonical registry without a cleared,
non-zero conflict-of-interest disclosure.

These transitions record eligibility and completion only. They do not move
treasury funds; signed consensus disbursement remains a later phase.

## Consensus boundary

The registry stores records under `community/v1/*` and maintains an O(1)
rolling root plus record counts. The root commits into the Trident state root.
Registration functions are state-machine library APIs only—there is no
unsigned mutation RPC or block lane yet. Existing forum/community RPC data
remains local administrative state and is excluded.
