# Agora community implementation phases

**Maturity:** Scaffold

## Purpose

C1–C23 are the community implementation phases for this readiness slice. Each status is one of **IMPLEMENTED**, **IN DEVELOPMENT**, or **PLANNED**, judged from code and tests on this commit. A status is IMPLEMENTED only when that phase's stated scope is present. Prototype scope is named in the evidence. Missing block admission, missing payouts, and missing certificates stay IN DEVELOPMENT or PLANNED.

This commit is the merge of the data-plane line, the architecture modules, and the docs slice. The banner in [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md) stays incomplete.

## Status

| ID | Phase | Status |
| --- | --- | --- |
| C1 | Separate TLT, OVL, and DRC consensus domains | IMPLEMENTED |
| C2 | Canonical community registry | IN DEVELOPMENT |
| C3 | Passport attestations | IN DEVELOPMENT |
| C4 | Reputation | IN DEVELOPMENT |
| C5 | Assembly and civic ballots | IN DEVELOPMENT |
| C6 | Missions | IN DEVELOPMENT |
| C7 | Academy | IN DEVELOPMENT |
| C8 | Grants | IN DEVELOPMENT |
| C9 | Bounties | IN DEVELOPMENT |
| C10 | Guilds | IN DEVELOPMENT |
| C11 | Merchant network | IN DEVELOPMENT |
| C12 | Events | IN DEVELOPMENT |
| C13 | Hubs | IN DEVELOPMENT |
| C14 | Protocol treasuries | IN DEVELOPMENT |
| C15 | Shared light-client verifier | IMPLEMENTED |
| C16 | Light-client security model | IN DEVELOPMENT |
| C17 | Phone light client | IMPLEMENTED |
| C18 | PC light client | IMPLEMENTED |
| C19 | Privacy model | IN DEVELOPMENT |
| C20 | Community API | IMPLEMENTED |
| C21 | Community indexer | IN DEVELOPMENT |
| C22 | PC and phone pairing | IMPLEMENTED |
| C23 | MY AGORA beginner home | IN DEVELOPMENT |

## Evidence

### C1 — IMPLEMENTED

TLT remains a UTXO asset, OVL an account and execution asset, and DRC a contract-free account asset. `apps/shared/light-client/featureMatrix.ts` keeps separate domains. Community records do not merge the three balances.

### C2 — IN DEVELOPMENT

`community_state.rs` initializes a summary, commits hubs, passports, grants, and missions under `community/v1/*`, and exposes `agora_getCommunityRegistry`. Tests cover registration rules inside the state-machine crate. [`community-registry.md`](community-registry.md) states there is no block lane and no unsigned mutation RPC.

### C3 — IN DEVELOPMENT

`PassportAttestation` is secp256k1-signed under `agora-passport-attestation-v1`, bound to chain id and genesis (`core/crates/crypto/src/passport.rs`). The wallet signs a community session challenge with the vault key. That session is not a canonical passport attestation. Revocation and consensus admission are open.

### C4 — IN DEVELOPMENT

Client reputation events ignore likes and reject badge transfer (`apps/shared/community/reputation.ts`). Scores shown in the passport screen come from the infrastructure catalog. They are not consensus state, and there is no consensus test that rejects buying a score.

### C5 — IN DEVELOPMENT

Assembly shows governance areas, eligibility, and an advisory badge when no chain commitment exists. `recordVote` throws. Civic RPC votes remain the existing administrative local flow. Caller-supplied weight is not rejected by a consensus ballot.

### C6 — IN DEVELOPMENT

`MissionRecord` enforces Open, Assigned, Completed, and Cancelled in `community_protocol.rs`. The infrastructure host can record the client state machine AVAILABLE through COMPLETED. Completion does not pay a treasury. The advance result sets `disbursesFunds: false`.

### C7 — IN DEVELOPMENT

The infrastructure catalog serves courses and lessons. Lesson progress is stored on the device and labeled local. `ACADEMY_CERTIFICATE` is **PLANNED**. Progress is not an on-chain certificate.

### C8 — IN DEVELOPMENT

`GrantRecord` binds one treasury, a beneficiary, a cap, and ordered milestone hashes. The released counter is record-keeping. The client constant `GRANT_DISBURSEMENT` is **PLANNED**. Milestone acceptance in the architecture module does not debit `TreasuryBalance`.

### C9 — IN DEVELOPMENT

Bounty rows are in the infrastructure catalog. `markBountyPaid` throws and `BOUNTY_DIRECTORY` stays **PLANNED**. A listed bounty is not a payment.

### C10 — IN DEVELOPMENT

Guild charters are catalog rows. `joinGuild` returns **PLANNED** and `member: false`. The charter object does not hold a member spend key. Joining is not a consensus operation.

### C11 — IN DEVELOPMENT

Merchant profiles store a DRC receiving address and `holdsMerchantKeys: false`. The pay screen verifies a listing and still shows destination and amount. A consensus merchant directory is open.

### C12 — IN DEVELOPMENT

Events are catalog rows. `claimAttendance` throws **PLANNED**. There is no signed attendance operation.

### C13 — IN DEVELOPMENT

`HubRecord` validates identity, charter hash, coordinators, multisig, periods, and accreditation status. The client can filter hubs by typed region. Signed accreditation and revocation operations are still deferred.

### C14 — IN DEVELOPMENT

Treasury rows come from `agora_getProtocolTreasuries` when the node answers. The community host has no `/treasury` route and does not fill a balance. `TreasurySpend` execution does not move canonical balances.

### C15 — IMPLEMENTED

Scope: the single-node prototype verifier in `apps/shared/light-client`. The verifier does not re-execute RandomX or check validator signatures. That limit is C16.

### C16 — IN DEVELOPMENT

`docs/core/LIGHT_CLIENT_SECURITY_MODEL.md` lists the assumptions. The device checks header hashes, selected-parent links, genesis binding, TLT Merkle inclusion, and quorum arithmetic on supplied stake. A dishonest but self-consistent node can still lie about the spine, RandomX work, and validator signatures.

### C17 — IMPLEMENTED

Scope: the Expo shell in `apps/mobile`, which imports the shared verifier, SecureStore vault, pairing, community screens, trust panel, architecture settings, and docs panel. It is the same single-node prototype as C15. Expo camera can fill the DRC QR field. Inspect still comes before signing.

### C18 — IMPLEMENTED

Scope: the desktop shell in `apps/desktop` (Vite and Tauri), which imports the same verifier, vault, pairing, community screens, trust panel, architecture settings, and docs panel. The PC wallet pastes a QR. It does not claim a camera.

### C19 — IN DEVELOPMENT

Private profile fields stay off consensus (`consensus: false`). A community session can read `/passport/private`. The profile is not a tested absence proof inside consensus encodings, and the JSON store is operator-readable.

### C20 — IMPLEMENTED

Scope: the experimental single-process host in `infrastructure/community-services`. It authenticates `POST /session` with a wallet signature, rejects mnemonic fields, and does not accept a pasted bearer as the proof. Catalog, forum replies, and mission reviews persist in the JSON file. That file is infrastructure trust, not a multi-user production service and not consensus RPC.

### C21 — IN DEVELOPMENT

`infrastructure/indexer` proxies an upstream full node and otherwise returns no chain rows. `chainProof` stays false. It does not emit blockchain, community, indexer, and private labels, and there is no test that fails when a private field is labeled blockchain.

### C22 — IMPLEMENTED

Scope: `apps/shared/light-client/pairing.ts`. Watch-only payloads carry public account data and omit the mnemonic. Restore payloads copy the BIP-39 phrase device to device after an explicit reveal. The phrase is not an RPC field.

### C23 — IN DEVELOPMENT

Settings on the PC and phone expose beginner and advanced copy: DRC PAYMENTS, OVL APPLICATIONS, TLT SECURITY/VALUE, and COMMUNITY PARTICIPATION. Those screens do not invent balances when RPC is down. They are not the default MY AGORA home.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | C1 and the consensus types behind C2, C3, C6, C8, C13, and C14. Only C1 is fully inside consensus admission. |
| community | C5 forum and ballots, C7 progress, C20's JSON host, and C4 scores shown from the catalog. |
| indexer | C21. Present as a proxy. Not a label classifier. |
| private | C19 and the vault half of C17, C18, and C22. Keys stay on device. |

## Trust boundary

Phase status is a documentation claim about this commit. It is not a consensus vote and not a maturity promotion. IMPLEMENTED rows are still Single-node prototype or an experimental infrastructure host. They are not a statement that the community ecosystem is finished.

## Maturity

Scaffold. The phase list is a reading aid over code that is itself Scaffold, Experimental, or Single-node prototype.

## Implemented

The IMPLEMENTED rows are C1, C15, C17, C18, C20, and C22, at the scopes in the evidence section.

## Planned

No row is only a name with zero code. Rows that still cannot pay, certify, vote, or admit a block stay IN DEVELOPMENT. Push transport, academy certificates, grant disbursement, bounty payment, guild join, and event attendance stay **PLANNED** inside those rows.

## Related

- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
