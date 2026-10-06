# Agora community definition of done

**Maturity:** Scaffold

COMMUNITY ECOSYSTEM — INCOMPLETE

## Purpose

This checklist is the definition of done for the community ecosystem. The ecosystem is complete only when every item below is true in code and in tests on the same commit. Checked items were confirmed by reading this base. Unchecked items are open. The banner stays **COMMUNITY ECOSYSTEM — INCOMPLETE** while any item is open.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Remote branches `cursor/agora-community-architecture-cdcf`, `cursor/agora-community-trust-cdcf`, and `cursor/agora-community-ecosystem-cdcf` were absent. Uncommitted files in other worktrees are outside this base.

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | Items about consensus admission, treasury debit, and asset separation refer to L1 state. |
| community | Items about the forum, ballots, and a future community API refer to off-consensus services or Meta-CF state. |
| indexer | The indexer item is open. These checkboxes are hand-written from the tree, not from an index. |
| private | Vault and pairing items refer to device-held keys. Private profile storage is open. |

## Trust boundary

A checked box means the named behavior is in this tree. It does not mean a third-party audit, a public network, or that neighboring unchecked items are close. Light-client boxes describe the single-node prototype. They do not claim header proofs for OVL or DRC state.

## Maturity

Scaffold. The checklist exists so readers can see the gap. Shipping the checklist does not finish the items.

## Implemented

The following items match this commit:

- [x] `apps/desktop` and `apps/mobile` import `apps/shared/light-client` and do not embed GHOSTDAG, RandomX, or a validator signer.
- [x] Watch-only pairing in `pairing.ts` builds a public payload. The mnemonic is not a field of that payload. Extended private key markers are rejected when a watch payload is parsed.
- [x] Same-spend restore can carry a BIP-39 phrase device to device. The phrase is not submitted on the node RPC. `agora-light-client.md` documents that path.
- [x] TLT, OVL, and DRC stay separate domains in `featureMatrix.ts` and in native balance reads.
- [x] `PassportAttestation` signatures use `agora-passport-attestation-v1` plus chain id and genesis. `community_state.rs` has register and list functions and no transfer function.
- [x] Grant and mission registration update community records. [`community-registry.md`](community-registry.md) states those transitions do not move treasury funds.
- [x] A Docs / About panel on the PC wallet, phone wallet, and explorer links the markdown files in this slice and does not call community RPC.

## Planned

The following items are not true on this commit. Several have partial code, recorded as IN DEVELOPMENT in [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md). Partial code does not check the box.

- [ ] Community hub, passport, grant, and mission registrations are admitted by a consensus block lane and covered by block tests.
- [ ] Passport revocation is public, signed, and tested, including replay and issuer checks.
- [ ] Reputation scores and badges exist in state, cannot be transferred, and tests reject purchase or conversion into spendable value.
- [ ] Assembly votes are signed by the voter key, bound to a consensus snapshot, and replicated. Caller-supplied vote weight is rejected.
- [ ] Mission completion pays the named treasury asset under a signed disbursement, with tests.
- [ ] Academy courses, lessons, and progress exist with tests, and progress storage is labeled private or community rather than blockchain.
- [ ] Grant milestone acceptance debits the bound treasury and cannot exceed the grant cap, with state tests.
- [ ] Bounties have state, claim rules, and tests.
- [ ] Guild charters have state and tests and do not hold member spend keys.
- [ ] Merchant profiles store a DRC receiving address only, and tests show the profile object has no spend key.
- [ ] Events have state and tests.
- [ ] Hub accreditation and revocation are signed consensus operations.
- [ ] `TreasurySpend` execution debits the canonical treasury balance for the named asset only.
- [ ] The light client locally checks RandomX, GHOSTDAG blue sets, and validator signatures, or it refuses to label those facts as locally verified. Today it refuses the label and still trusts one node for those facts, so this item stays open.
- [ ] A community HTTP API on this commit authenticates with a wallet signature, never accepts a mnemonic, and has tests.
- [ ] An indexer emits blockchain, community, indexer, and private labels, and tests fail when a private field is labeled blockchain.
- [ ] Private passport fields have an authenticated store, and a test shows they are absent from consensus encodings.
- [ ] A MY AGORA beginner home exists in a client, shows the four-role orientation as copy, and does not invent balances when RPC is down.
- [ ] Every item in this file is true, and this banner is updated only in that commit.

## Related

- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Vision and planned home: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
