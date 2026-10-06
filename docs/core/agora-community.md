# Agora community

**Maturity: Experimental.**

Community is how people meet around Agora. It is not a fourth asset. DRC
moves value, OVL builds value, and TLT secures value. The PC and phone apps
remain light clients: they keep headers, Merkle checks, and wallet signatures
on the device. They do not run a full node, and they do not mine.

This slice sits beside the canonical registry in
[`community-registry.md`](community-registry.md) and the administrative civic
RPC in [`governance.md`](governance.md). It does not turn either of those into
consensus.

## What is separate

| Thing | Where it lives | What it is not |
| --- | --- | --- |
| Wallet key | Device vault | A community session |
| Community session | Short-lived bearer token after a wallet signature | A seed or password |
| Public passport | Username, avatar hash, roles, category reputation, badges, contribution counts | A financial token or transferable NFT |
| Private profile | Email and language, on device or behind the session | Consensus state or a KYC document |
| Advisory poll | Community service | An on-chain execution |
| On-chain governance | Only when a commitment id is indexed from a full node | A light-client proof of validator signatures |

Every field shown in the clients carries one source label: on-chain verified,
cryptographically verified, indexed, community submitted, or external.

## Reputation

Scores move only for verifiable events: a completed mission, an accepted grant
milestone, a recorded vote, or an academy certificate. A forum like does not
move a score. Badges are non-transferable. There is no automatic token reward
for an action.

Anti-Sybil for normal use is a wallet signature, a rate limit, and a
reputation threshold. This client does not collect identity documents.

## QR

Payloads are version 1 and one of: `drc-payment`, `merchant-pay`, `address`,
`passport-share`, `event`, `mission`, `grant`, `profile`.

```text
agora:1:drc-payment?to=<address>&amount=<positive-integer>
```

A missing version, an unknown kind, a duplicate field, a payment without a
destination and amount, or a payload that tries to retarget the asset fails
closed. The pay screen shows destination and amount before it will sign.
Merchant profiles store a receiving address only.

Consensus DRC payment bytes are versioned in the node. This light client does
not reconstruct them, so the broadcast step reports `unavailable` and
`confirmed: false`. Nothing is submitted in place of a real payment.

## Offline

Passport, academy, and docs can be cached on the device. An offline screen
says it is a cached view and never says the data is confirmed.

## Clients

PC nav: HOME, WALLET, DRC, OVL, TLT, SWAP, ACTIVITY, PASSPORT, COMMUNITY,
MISSIONS, ACADEMY, GRANTS, BOUNTIES, GUILDS, MERCHANTS, EVENTS, ASSEMBLY,
TREASURY, EXPLORER, SETTINGS.

Phone primary nav: HOME, WALLET, COMMUNITY, ACTIVITY, PASSPORT. Secondary:
DRC, OVL, TLT, SWAP, MISSIONS, ACADEMY, MERCHANTS, EVENTS, ASSEMBLY, GRANTS,
GUILDS, BOUNTIES, SETTINGS. Home leads with Passport. SWAP stays unavailable
because there is no DEX or bridge. The phone does not mine.

Hub search is a typed region or name. The clients do not request GPS.
Notification preferences can mention a mission or poll. Push copy strips
amounts.

Assembly shows governance areas with eligibility labels. A passport with only
Community reputation is not eligible for the OVL / EVM area. Civic RPC votes
remain the existing administrative local flow and are labeled as such.

Treasury rows prefer `agora_getProtocolTreasuries` when the node answers.
Those balances are node-reported. They are not a header proof, and unsigned
civic RPC still cannot spend them.

## API

`apps/community-api` serves the same types as `apps/shared/community`:

`/passport` `/passport/private` `/reputation` `/missions` `/grants`
`/bounties` `/academy` `/events` `/merchants` `/guilds` `/proposals`
`/treasury` `/contributions` plus `/hubs` `/forum` `/developers` `/ecosystem`
and `POST /session`.

Without `VITE_AGORA_COMMUNITY_URL` or `EXPO_PUBLIC_AGORA_COMMUNITY_URL`, the
clients use the local fixture bundle and label it community submitted, not
confirmed. The dev server keeps sessions in memory and does not store seeds.

## Still out of this slice

- Camera QR capture.
- Constructing and submitting a consensus `DrcPaymentTx` from the community pay screen (the light-client envelope builder exists separately and is not a confirmation).
- Delivering phone push notifications.
- A durable multi-user community service.
- Signed, block-replicated governance and treasury disbursement.
- Issuing canonical passport attestations from the wallet.

The architecture slice below does not replace the product modules in
`apps/shared/community`. It adds stores, service boundaries, folder re-exports,
and security hooks. Threat model:
[`LIGHT_CLIENT_SECURITY_MODEL.md`](LIGHT_CLIENT_SECURITY_MODEL.md).

## 48. Data architecture

Blockchain storage is only for facts the device can recompute or that
consensus has committed. Everything else has an explicit store.

| Store | Holds | Does not hold |
| --- | --- | --- |
| BLOCKCHAIN | Header hashes, TLT Merkle proofs, signed transaction bytes, committed registry roots | Display names, likes, notification toggles, node-reported balances |
| COMMUNITY | Passport profiles, drafts, forum text, merchant directory copy, fixture bundles labeled community submitted | Keys, finality, payment receipts |
| INDEXER | Node-reported balances, list order, treasury rows from `agora_getProtocolTreasuries`, service mirrors | Spend authorization, header proofs |
| USER PRIVATE | Mnemonic, vault password, PIN hash, RPC token, spend-session secret | Anything sent to a community API |
| CACHE | Tip snapshots, offline views, QR matrices | A claim that cached data is confirmed |
| NOTIFICATIONS | On-device preferences and amount-stripped push copy | Chain events |

`placementFor` in `apps/shared/core/stores.ts` is the map. A mnemonic cannot be
placed in BLOCKCHAIN.

## 49. Backend

`apps/shared/core/services.ts` defines in-process adapters for Community API,
Identity/Passport, Mission, Grant, Bounty, Academy, Event, Merchant, Forum,
Notification, and Indexing. Each adapter documents what it can lie about and
what it must not claim. Reads return an empty **PLANNED** list with
`fabricated: false`. Mutations are not accepted.

`apps/community-api` remains the experimental HTTP adapter from the product
slice. Its fixture mode can lie about community-submitted records. It still
must not claim those records are on-chain verified, and it must not store seeds.

## 50. Client modules

`apps/shared/moduleMap.ts` lists the folders. `community` stays the existing
product module so pairing and the community screens keep their imports. The
other folders re-export that module and the light client instead of moving
files:

`core` `wallet` `lightclient` `drc` `ovl` `tlt` `passport` `community`
`missions` `academy` `grants` `bounties` `guilds` `merchants` `events`
`assembly` `treasury` `forum` `notifications` `explorer` `security` `settings`.

Desktop and phone settings import `ArchitecturePanel` from those folders.

## 51. Mobile security

SecureStore keeps the sealed vault blob. PIN records store a salted hash and
`holdsSeed: false`. Hardware biometrics, secure-element keys, and native
screenshot blocking (iOS and Android) are **PLANNED**. The mnemonic stays
hidden until Show. Clipboard copy of a seed requires an explicit reveal.
Phishing-resistant confirmations use the transaction preview: destination and
amount are shown, and `signed` stays false until the preview id is confirmed.

## 52. PC security

The vault remains AES-256-GCM in `localStorage`. Version 1 blobs still open.
A stronger KDF is **PLANNED**. The desktop wallet locks after inactivity
(default five minutes) and clears the mnemonic from memory. Node-address
warnings cover plaintext HTTP, localhost, embedded credentials, punycode, and
a host that does not match the saved endpoint. `HardwareSigner` is the USB
wallet hook and returns **PLANNED** because no USB signer is linked. Signing
goes through `signAfterPreview`.

## 53–55. Beginner and advanced

Beginner surfaces are create wallet, import wallet, Passport, receive DRC,
pay, community, first mission, Academy, merchants, projects, and join Guild.
Projects stay **PLANNED**. Beginner copy is `DRC PAYMENTS`, `OVL APPLICATIONS`,
`TLT SECURITY/VALUE`, and `COMMUNITY PARTICIPATION`, without consensus jargon.

Advanced surfaces are RPC, raw transactions, UTXO details, proofs, network,
and developer tools. Contract calls are **PLANNED** because OVL-EVM is not on
this base. Nonempty calldata is rejected.

## 56–57. Explorer integration

`apps/shared/explorer/links.ts` points DRC, OVL, and TLT transactions at the
explorer `#tx` section, blocks at `#live`, and treasury rows at `#trident`,
and only when the caller already has a real id. Contracts, tokens, NFTs, DEX,
and AMM links are **PLANNED**.

Community trails:

- Grant → funding transaction → treasury → recipient → Passport. A missing
  funding transaction is **PLANNED**. A grant id is not a disbursement.
- Merchant → DRC address → payments → profile. Profiles and sales stay
  **PLANNED**.
- Developer → Passport → projects → contracts → grants → missions. Projects
  and contracts are **PLANNED**.
- Miner → public stats → TLT contribution. Miner stats are **PLANNED**. A
  real TLT transaction id can open `#tx`.

## 58. Implementation rule

No mock confirmations, governance outcomes, reputation proofs, or merchant
activity are added in this slice. Cache views keep `confirmed: false`. The
community pay path keeps `broadcast: "unavailable"` and `confirmed: false`.
In-process services return no records. Likes do not move reputation. A
governance badge without a chain commitment stays advisory. Bounty payment,
event attendance, guild join, academy certificates, and treasury spend stay
**PLANNED**.

## 59. Tests and security notes

`apps/shared/architecture/architecture.test.ts` covers passport signatures and
nonce replay, reputation events that ignore likes, mission transitions,
grant milestones that do not disburse, empty bounties, advisory governance
badges, merchant QR parse and seal tampering, notification preferences,
TLT Merkle proofs, wallet vault and spend-session integration, DRC/OVL/TLT
builders present and contract calls **PLANNED**, authorization, privacy,
rate limits with Sybil resistance still **PLANNED**, and a locked session
that cannot export a seed.

```bash
cd apps/shared
node --experimental-strip-types --import ./light-client/register-ts-ext.mjs architecture/architecture.test.ts
```

Security notes for operators are in
[`LIGHT_CLIENT_SECURITY_MODEL.md`](LIGHT_CLIENT_SECURITY_MODEL.md).
