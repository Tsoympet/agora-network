# Agora community definition of done

**Maturity:** Scaffold

COMMUNITY ECOSYSTEM — INCOMPLETE

## Purpose

This checklist is the definition of done for the community ecosystem. The ecosystem is complete only when every item below is true in code and in tests on the same commit. Checked items were confirmed by reading this tree after the data-plane, architecture, and docs stacks were merged. Unchecked items are open. The banner stays **COMMUNITY ECOSYSTEM — INCOMPLETE** while any item is open.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Items about consensus admission, treasury debit, and asset separation refer to L1 state. |
| community | Forum replies, the JSON catalog, and advisory polls are infrastructure or device records. |
| indexer | The indexer is an optional upstream proxy. It does not emit the four source labels as a tested classifier. |
| private | Vault keys, pairing payloads, and the in-app notification inbox stay on the device. |

## Trust boundary

A checked box means the named behavior is in this tree. It does not mean a third-party audit, a public network, or that neighboring unchecked items are close. Light-client boxes describe the single-node prototype. They do not claim header proofs for OVL or DRC state. The community JSON file is infrastructure trust: the operator can edit it.

## Maturity

Scaffold. The checklist exists so readers can see the gap. Shipping the checklist does not finish the items.

## Implemented

The following items match this commit:

- [x] `apps/desktop` and `apps/mobile` import `apps/shared/light-client` and do not embed GHOSTDAG, RandomX, or a validator signer.
- [x] Watch-only pairing in `pairing.ts` builds a public payload. The mnemonic is not a field of that payload. Extended private key markers are rejected when a watch payload is parsed. Watch-only mode cannot sign a community spend.
- [x] Same-spend restore can carry a BIP-39 phrase device to device. The phrase is not submitted on the node RPC. `agora-light-client.md` documents that path.
- [x] TLT, OVL, and DRC stay separate domains in `featureMatrix.ts` and in native balance reads.
- [x] `PassportAttestation` signatures use `agora-passport-attestation-v1` plus chain id and genesis. `community_state.rs` has register and list functions and no transfer function.
- [x] Grant and mission registration update community records. [`community-registry.md`](community-registry.md) states those transitions do not move treasury funds. The pay and grant screens do not invent a disbursement.
- [x] One client tree mounts Passport, reputation, missions, Academy, grants, bounties, guilds, merchants, events, hubs, assembly, treasury, forum, notifications, pairing, data-plane labels, architecture modules, beginner and advanced settings, the trust panel, and the docs panel.
- [x] The wallet signs the community session challenge with the vault key. `openSession` rejects a pasted bearer token. The infrastructure host rejects a mnemonic on `POST /session`.
- [x] DRC QR keeps paste. The phone can scan with Expo camera. Destination and amount stay on screen before signing. `agora_submitDrcPayment` is used only when the node exposes it. A receipt stays unconfirmed until `agora_getDrcPayment` returns a receipt object.
- [x] The forum reply composer posts to the infrastructure host. The device does not import `infrastructure/forum-server` or `infrastructure/indexer`.
- [x] The infrastructure process stores the catalog, forum replies, and mission reviews in a JSON file. [`AGORA_DATA_PLANE_SPLIT.md`](AGORA_DATA_PLANE_SPLIT.md) calls that file infrastructure trust.
- [x] Notification preferences keep amount stripping. There is no APNs or FCM client. The in-app inbox is the delivery path and push transport stays **PLANNED**.
- [x] Academy certificates, grant disbursements, and governance vote results stay **PLANNED** or advisory. The UI does not invent those outcomes.
- [x] [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md) lists C1–C23 against this tree.
- [x] A Docs / About panel on the PC wallet, phone wallet, and explorer links the markdown files in this slice and does not call community RPC.

## Planned

The following items are not true on this commit. Partial code does not check the box.

- [ ] Community hub, passport, grant, and mission registrations are admitted by a consensus block lane and covered by block tests.
- [ ] Passport revocation is public, signed, and tested, including replay and issuer checks.
- [ ] Reputation scores and badges exist in consensus state, cannot be transferred, and tests reject purchase or conversion into spendable value.
- [ ] Assembly votes are signed by the voter key, bound to a consensus snapshot, and replicated. Caller-supplied vote weight is rejected.
- [ ] Mission completion pays the named treasury asset under a signed disbursement, with tests.
- [ ] Academy courses, lessons, and progress exist in a store other than the operator JSON catalog and device cache, and a certificate is issued only by a signed rule.
- [ ] Grant milestone acceptance debits the bound treasury and cannot exceed the grant cap, with state tests.
- [ ] Bounty claims pay a treasury under a signed rule, with tests. Listing a bounty is not a payment.
- [ ] Guild charters are consensus objects. Joining a guild does not hold member spend keys, and the join is not only a **PLANNED** client return.
- [ ] Merchant profiles are consensus objects. Tests already show the profile object has no spend key, and that is not the same as a consensus directory.
- [ ] Event attendance is a signed consensus operation. A client claim stays **PLANNED**.
- [ ] Hub accreditation and revocation are signed consensus operations.
- [ ] `TreasurySpend` execution debits the canonical treasury balance for the named asset only.
- [ ] The light client locally checks RandomX, GHOSTDAG blue sets, and validator signatures, or it refuses to label those facts as locally verified. Today it refuses the label and still trusts one node for those facts, so this item stays open.
- [ ] An indexer emits blockchain, community, indexer, and private labels, and tests fail when a private field is labeled blockchain.
- [ ] A MY AGORA beginner home is the default client shell. Beginner and advanced settings exist and do not invent balances, and that is not this item.
- [ ] Every item in this file is true, and this banner is updated only in that commit.

## Related

- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Vision and planned home: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
