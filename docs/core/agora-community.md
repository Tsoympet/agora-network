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
- Constructing and submitting a consensus `DrcPaymentTx`.
- Delivering phone push notifications.
- A durable multi-user community service.
- Signed, block-replicated governance and treasury disbursement.
- Issuing canonical passport attestations from the wallet.
