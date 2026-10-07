# Canonical community registry

**Maturity:** Experimental for signed passport attestations; Scaffold for
Hub/Grant/Mission library registration.

Phase 5b defines canonical schemas and a bounded state commitment for Agora
Hubs, Passport attestations, Grants, and Missions. The registry is initialized
at genesis and exposed through `agora_getCommunityRegistry`.

## Hubs

Canonical Hub records include a stable ID, public name, geographic/specialist
classification, charter hash, unique coordinators, accreditation proposal, and
status. Active Hubs require a canonical accreditation proposal, non-zero
multisig address, election/reporting periods, and COI commitment.

`HubRecord` has no secp256k1 envelope, so hub accreditation stays a genesis /
library write (`register_hub_into`). There is no unsigned hub mutation RPC.

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
treasury funds; signed consensus disbursement remains a later phase. Grant
and Mission records have no signed mutation envelope, so they stay library
APIs (`register_grant_into`, `register_mission_into`). No unsigned mutation
RPC is exposed.

## Consensus boundary

The registry stores records under `community/v1/*` and maintains an O(1)
rolling root plus record counts. The root commits into the Trident state root.

Passport attestations are the signed community mutation that can be admitted
on default Experimental boot. Hub, Grant, and Mission writes remain library
or genesis until those families gain their own secp256k1 envelopes. Civic
forum/vote RPC stays local administrative state and is excluded from this
lane.
