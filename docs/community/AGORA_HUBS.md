# Agora Hubs

**Maturity:** Scaffold.

Hubs are geographic or specialist communities. No hub permanently owns an exclusive geographic territory.

## Example hubs

Agora Athens, Thessaloniki, Cyprus, Europe, Developers, Miners, Validators, Merchants, Universities, Creators.

## Hub record

| Field | Description |
| --- | --- |
| Hub ID | Stable identifier |
| Public name | Display name |
| Region or specialty | Classification |
| Public charter hash | Content-addressed charter |
| Multisignature treasury | No single-key control |
| Coordinators | Current operators |
| Election term | Coordinator term length |
| Reporting schedule | Public report cadence |
| Accreditation state | Pending / active / suspended / revoked |
| COI disclosures | Required |
| Public deliverables | Linked artifacts |
| Revocation process | Documented offboarding |

Accreditation and revocation are governance-auditable (Ecclesia + DRC Community per constitution matrix).

The canonical scaffold validates and commits Hub identity, charter,
coordinators, multisig, election/reporting periods, COI and deliverable roots,
accreditation proposal, and status. Active Hub coordinators form the O(1)
Passport issuer index; signed accreditation/revocation operations remain
deferred.
