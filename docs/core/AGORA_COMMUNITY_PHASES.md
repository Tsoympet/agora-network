# Agora community implementation phases

**Maturity:** Scaffold

## Purpose

C1–C23 are the community implementation phases for this readiness slice. Each status is one of **IMPLEMENTED**, **IN DEVELOPMENT**, or **PLANNED**, judged from code and tests on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). A status is IMPLEMENTED only when that phase's stated scope is present on this commit. Prototype scope is named in the evidence. Missing block admission, missing payouts, and missing screens stay IN DEVELOPMENT or PLANNED.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Remote branches `cursor/agora-community-architecture-cdcf`, `cursor/agora-community-trust-cdcf`, and `cursor/agora-community-ecosystem-cdcf` were absent. Uncommitted files in other worktrees are outside this base.

Remote community architecture, trust, and ecosystem branches were not available to stack on. Local worktrees of those names pointed at the same commit, and one held uncommitted community client files. Those files are not on this commit, so their modules stay PLANNED here.

## Status

| ID | Phase | Status |
| --- | --- | --- |
| C1 | Separate TLT, OVL, and DRC consensus domains | IMPLEMENTED |
| C2 | Canonical community registry | IN DEVELOPMENT |
| C3 | Passport attestations | IN DEVELOPMENT |
| C4 | Reputation | PLANNED |
| C5 | Assembly and civic ballots | IN DEVELOPMENT |
| C6 | Missions | IN DEVELOPMENT |
| C7 | Academy | PLANNED |
| C8 | Grants | IN DEVELOPMENT |
| C9 | Bounties | PLANNED |
| C10 | Guilds | PLANNED |
| C11 | Merchant network | PLANNED |
| C12 | Events | PLANNED |
| C13 | Hubs | IN DEVELOPMENT |
| C14 | Protocol treasuries | IN DEVELOPMENT |
| C15 | Shared light-client verifier | IMPLEMENTED |
| C16 | Light-client security model | IN DEVELOPMENT |
| C17 | Phone light client | IMPLEMENTED |
| C18 | PC light client | IMPLEMENTED |
| C19 | Privacy model | PLANNED |
| C20 | Community API | PLANNED |
| C21 | Community indexer | PLANNED |
| C22 | PC and phone pairing | IMPLEMENTED |
| C23 | MY AGORA beginner home | PLANNED |

## Evidence

### C1 — IMPLEMENTED

TLT remains a UTXO asset, OVL an account and execution asset, and DRC a contract-free account asset. `apps/shared/light-client/featureMatrix.ts` keeps separate domains. Community records do not merge the three balances.

### C2 — IN DEVELOPMENT

`community_state.rs` initializes a summary, commits hubs, passports, grants, and missions under `community/v1/*`, and exposes `agora_getCommunityRegistry`. Tests cover registration rules inside the state-machine crate. [`community-registry.md`](community-registry.md) states there is no block lane and no unsigned mutation RPC.

### C3 — IN DEVELOPMENT

`PassportAttestation` is secp256k1-signed under `agora-passport-attestation-v1`, bound to chain id and genesis (`core/crates/crypto/src/passport.rs`). Issuers must be active hub coordinators, and issuer nonces are stored. Revocation, a wallet screen, and consensus admission are open.

### C4 — PLANNED

[`docs/community/AGORA_PASSPORT.md`](../community/AGORA_PASSPORT.md) names contribution reputation and separates it from stake and token balances. This commit has no reputation score state and no badge transfer tests.

### C5 — IN DEVELOPMENT

`agora-governance` has chambers, proposal lifecycle, forum topics, and JSON-RPC. Desktop `GovernancePanel` and the explorer ballot section call that RPC. [`governance.md`](governance.md) classifies the engine as an administrative prototype: unsigned votes, caller-supplied balances, local Meta-CF persistence.

### C6 — IN DEVELOPMENT

`MissionRecord` enforces Open, Assigned, Completed, and Cancelled in `community_protocol.rs`. `register_mission_into` commits the record. Completion does not pay a treasury. There is no mission screen.

### C7 — PLANNED

Academy is a paragraph in [`docs/community/COMMUNITY_PROGRAMS.md`](../community/COMMUNITY_PROGRAMS.md). No course state and no client catalog exist on this commit.

### C8 — IN DEVELOPMENT

`GrantRecord` binds one treasury, a beneficiary, a cap, and ordered milestone hashes. DRC Community grants require a cleared conflict-of-interest disclosure before registration. The released counter is record-keeping. It does not debit `TreasuryBalance`.

### C9 — PLANNED

Bounties and RFPs are named in [`docs/community/GRANTS_AND_MISSIONS.md`](../community/GRANTS_AND_MISSIONS.md). No bounty type or test exists on this commit.

### C10 — PLANNED

Builder Guild and Node Guild are named in `COMMUNITY_PROGRAMS.md`. No guild charter state exists on this commit.

### C11 — PLANNED

[`docs/community/MERCHANT_NETWORK.md`](../community/MERCHANT_NETWORK.md) is a scaffold. DRC payment, escrow, check, and channel types exist on the L1 and as light-client envelope actions. A merchant directory, map, invoice product, and receiving-profile store do not.

### C12 — PLANNED

Events are listed in `COMMUNITY_PROGRAMS.md`. No event record exists on this commit.

### C13 — IN DEVELOPMENT

`HubRecord` validates identity, charter hash, coordinators, multisig, periods, and accreditation status. Active coordinators form the passport issuer index. Signed accreditation and revocation operations are still deferred, and there is no hub directory screen.

### C14 — IN DEVELOPMENT

`TreasuryId` is TLT Security, OVL Builder, and DRC Community, with balances initialized in governance state. `ProposalKind::TreasurySpend` can pass the civic engine in tests. Execution does not move those canonical balances. See [`governance.md`](governance.md).

### C15 — IMPLEMENTED

Scope: the single-node prototype verifier in `apps/shared/light-client`, with tests such as `agora-light.test.ts`, `featureMatrix.test.ts`, `tlt-bitcoin.test.ts`, and `pairing.test.ts`. The verifier does not re-execute RandomX or check validator signatures. That limit is C16.

### C16 — IN DEVELOPMENT

The device checks header hashes, selected-parent links, genesis binding, TLT Merkle inclusion, body-root binding, and the two-thirds quorum arithmetic when stake fields are present. A dishonest but self-consistent node can still lie about the spine, balances, and stake totals. `docs/core/LIGHT_CLIENT_SECURITY_MODEL.md` is not in this tree. The node-wide note is [`docs/security/THREAT_MODEL.md`](../security/THREAT_MODEL.md).

### C17 — IMPLEMENTED

Scope: the Expo shell in `apps/mobile`, which imports the shared verifier, SecureStore vault, pairing, and capability panel. It is the same single-node prototype as C15.

### C18 — IMPLEMENTED

Scope: the desktop shell in `apps/desktop` (Vite and Tauri), which imports the same verifier, `localStorage` vault, pairing, capability panel, and civic ballot panel.

### C19 — PLANNED

Public passport fields are the attestation struct. A private profile (email, language, notification preferences) with an authenticated store and tests that the fields stay out of consensus is not on this commit.

### C20 — PLANNED

No `apps/community-api` (or equivalent) is in this commit. A session-signed community HTTP service is future work. It must stay off the consensus RPC and must not accept seeds.

### C21 — PLANNED

No community indexer is in this commit. Explorer panels call the node. Source labels in these documents are author labels, not an indexer's output.

### C22 — IMPLEMENTED

Scope: `apps/shared/light-client/pairing.ts`. Watch-only payloads carry public account data and omit the mnemonic. Restore payloads copy the BIP-39 phrase device to device after an explicit reveal. The phrase is not an RPC field. Loopback RPC URLs are left out of the watch payload.

### C23 — PLANNED

Desktop, phone, and explorer have wallet, capability, ballot, and DAG screens. None of them is a MY AGORA beginner home. The target layout is in [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md). This slice adds a Docs / About index only.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | C1, C2, C3, C6, C8, C13, and C14 touch L1 types or state-machine records. Only C1 is fully inside consensus admission today. |
| community | C5 forum and ballots are local administrative state. C20 would be a community service. |
| indexer | C21. Absent on this commit. |
| private | C19 and the vault half of C17, C18, and C22. Keys stay on device. Profile storage is PLANNED. |

## Trust boundary

Phase status is a documentation claim about this commit. It is not a consensus vote and not a maturity promotion. IMPLEMENTED rows are still Single-node prototype or consensus code that this repo already shipped in earlier PRs. They are not a statement that the community ecosystem is finished. The checklist file carries the overall result.

## Maturity

Scaffold. The phase list is a reading aid over code that is itself Scaffold, Experimental, or Single-node prototype.

## Implemented

The IMPLEMENTED rows are C1, C15, C17, C18, and C22, at the scopes in the evidence section.

## Planned

The PLANNED rows are C4, C7, C9, C10, C11, C12, C19, C20, C21, and C23. IN DEVELOPMENT rows have partial code and stay open in the definition of done.

## Related

- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
