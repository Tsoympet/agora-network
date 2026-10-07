# Canonical community registry

**Maturity:** Experimental for signed passport attestations and signed
Hub / Grant / Mission registrations.

Phase 5b defines canonical schemas and a bounded state commitment for Agora
Hubs, Passport attestations, Grants, and Missions. The registry is initialized
at genesis and exposed through `agora_getCommunityRegistry`.

## Hubs

Canonical Hub records include a stable ID, public name, geographic/specialist
classification, charter hash, unique coordinators, accreditation proposal, and
status. Active Hubs require a canonical accreditation proposal, non-zero
multisig address, election/reporting periods, and COI commitment.

Hub accreditation is a signed consensus lane. `HubRegistration` is
secp256k1-signed by the first coordinator under `agora-hub-registration-v1`.
Apply creates an Active `HubRecord` so those coordinators can issue
passports, grants, and missions. Coordinator nonces prevent replay. Genesis
and library `register_hub_into` remain for Block 0 / tests. There is no
unsigned hub mutation RPC.

## Passport

Passport attestations are secp256k1-signed under
`agora-passport-attestation-v1`. The signature binds issuer, subject, category,
evidence, issuer policy, epochs, nonce, chain ID, and genesis hash. Changing the
subject invalidates the signature; there is no transfer API.

Only coordinators indexed from active canonical Hubs may issue attestations.
Issuer nonces prevent replay. This is contribution evidence, not a claim of
complete Sybil resistance or verified personhood.

The signed passport is a consensus lane: block body field, UTXO journal
revert, mempool reservation (one pending nonce per issuer), gossip
`NetworkMessage::PassportAttestation` (protocol v29, discriminant 37), typed
compact lane 30, `agora_submitPassportAttestation` /
`agora_getPassportAttestation` / `agora_getPassportIssuerNonce`, and a
device-local light-client builder. Apply hard-fails without an active hub
coordinator or matching issuer nonce.

## Grants and Missions

Grant schemas bind a governance proposal, one asset-fixed protocol treasury,
beneficiary, total, and ordered milestones. Milestone acceptance requires the
next exact deliverable hash and cannot exceed the grant cap. Mission schemas
enforce Open → Assigned → Completed transitions with completion evidence.
DRC Community grants cannot enter the canonical registry without a cleared,
non-zero conflict-of-interest disclosure.

These transitions record eligibility and completion only. They do not move
treasury funds; signed consensus disbursement remains a later phase.

Grant and Mission **registration** is a signed consensus lane.
`GrantRegistration` is signed by an active hub coordinator
(`agora-grant-registration-v1`). `MissionRegistration` is signed by an
active hub coordinator acting as sponsor (`agora-mission-registration-v1`).
Both use dedicated nonces. Library `register_grant_into` /
`register_mission_into` remain for genesis / tests. No unsigned mutation
RPC is exposed.

## Consensus boundary

The registry stores records under `community/v1/*` and maintains an O(1)
rolling root plus record counts. The root commits into the Trident state root.

Passport attestations and Hub / Grant / Mission registrations are the signed
community mutations admitted on default Experimental boot (protocol v30,
discriminants 38–40, compact lanes 31–33). Civic forum/vote RPC stays local
administrative state and is excluded from this lane.
