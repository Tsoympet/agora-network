# Native DRC order book

**Maturity:** Experimental

This is a protocol-native direct order book for DRC. It is an Agora adaptation
of XRPL Offer / OfferCreate / OfferCancel semantics. It is not XRPL wire,
hash, reserve, amendment, or API parity, and it is not an AMM or an EVM DEX.

## Objects and transactions

`DrcOfferCreateTx` and `DrcOfferCancelTx` are canonical state transitions.
Offer ids are `SHA-256` of the Borsh-encoded create transaction. A resting
`DrcOfferLive` is stored only when a good-till-cancel remainder remains
positively funded.

- `taker_gets` is what the owner sells and what a taker receives.
- `taker_pays` is what the owner buys and what a taker pays.
- Amounts are `u64`. Quality and fills use checked `u128` cross-multiplication.
  There is no floating point.
- Native DRC is `DrcBookAsset::NativeDrc`. An issued asset is
  `DrcBookAsset::Issued(IssuedAssetId)`. The same currency code from two
  issuers is two assets. A book whose two sides are the same asset is rejected,
  including native DRC against native DRC.

Books are indexed by the directed pair. Offers on one book are ordered by
original `taker_gets / taker_pays` (higher is better), then by the monotonic
`book_sequence` assigned when the remainder is placed, then by offer id.
`book_sequence` lives at `offer/drc/sequence`.

## Crossing

A new offer crosses a resting maker when

```text
taker_gets * maker_gets >= maker_pays * taker_pays
```

using each offer's remaining amounts. Execution uses the maker's rate:

```text
out = min(want, floor(budget * maker_gets / maker_pays))
in  = ceil(out * maker_pays / maker_gets)
```

`None` from that step is dust: the maker is removed and its locked funds are
released. Buy is complete when `taker_pays` remaining is zero. Sell is complete
when `taker_gets` remaining is zero.

Time in force:

- Good-till-cancel places a still-funded positive remainder.
- Immediate-or-cancel never places a remainder. A zero-fill create is still
  `Accepted` when the owner can fund a positive sell, and the signed fee burns.
- Fill-or-kill rejects the whole transaction before any mutation when the
  requested amount is not fully filled. Sequence is not consumed and no fee
  burns.

Before other makers are considered, the engine cancels the owner's opposite
offers that cross the new offer, regardless of size. Those cancellations count
toward the match cap and are not executed against the new offer.

The walk stops at the first resting maker that does not cross. Expired offers
later in that book are not swept until a later walk reaches them. Expiration
is inclusive through `expires_after_blue_score`; an offer is expired when the
application blue score is greater. Creating an already-expired offer is a
hard reject.

## Funding and issuer controls

Native sells debit `taker_gets_remaining` from the spendable DRC balance at
placement (`native_locked`). Issued holder sells add that remainder to
`offer/drc/reserve/{holder}{asset}`. The issuer of that issued asset may sell
without a holder balance and mints units on fill. Native DRC is never an
issued asset and is never subject to issuer controls.

Unfunded placement is a hard reject: no `tec` class, no sequence consumption,
and no fee burn. Crossing rechecks funding. Expired or permanently unfunded
makers are removed. A maker skipped because the counterparty cannot receive,
or because a freeze blocks that pair, stays in the book when the maker is the
issuer or still holds a positive authorized balance that is only temporarily
blocked. Deep-frozen or unauthorized holder sells are removed when walked, and
a new deep-frozen or unauthorized holder sell is rejected.

No implicit trust line is created. Issued transfers and clawbacks cannot spend
units reserved by a live offer. Those lanes run before offers in the same
block, so a same-block clawback can take units before a later offer reserves
them.

RequireAuth, global freeze, line freeze, and deep freeze apply only to the
issued asset being moved. A frozen issued balance does not freeze the account's
native DRC.

## Authorization, fees, and bounds

Offer create and cancel use the shared DRC authorization path:

- master key, one regular key, or weighted multisign;
- master disable rejects the master signature while regular-key or multisign
  recovery remains;
- version 1 selects the account nonce;
- version 2 selects one Ticket, which is consumed once.

Signing domains are `agora-trident-drc-offer-create-v1` / `v2` and
`agora-trident-drc-offer-cancel-v1` / `v2`. Multisign operation kinds are 24
and 25.

The signed DRC fee is debited inside the operation and burned only when the
operation is `Accepted`, including an idempotent cancel of a missing offer
(`Absent`). Rejected operations burn zero.

Caps, all fail-closed before the transaction mutates state:

| Bound | Value |
| --- | --- |
| Live offers per account | 32 |
| Live offers per book | 256 |
| Match steps per transaction | 16 |
| Match steps per block | 64 |
| Query page | 32 |

## Consensus surface

Offer lanes are appended after the multisign attachment lane. Empty lanes are
omitted from Borsh so earlier bodies still decode. A non-empty lane commits
`agora-block-body-v18`. A later non-empty TLT covenant lane wraps that inner
root as `agora-block-body-v19` so the two experimental trailing slots cannot
collide. The offer root `agora-drc-offer-root-v1` is a
component of `agora-trident-state-root-v16`. Protocol version is 24 and the
state transition is `agora-trident-state-v22`. The datadir schema stays 21
because the new object kind fits the existing ledger-object index.

The mempool reserves the shared nonce or Ticket, one pending cancel per offer
id, the native sell plus fee, and the issued sell reserve when the owner is
not the issuer. Other lanes' spends are not summed into that reservation;
canonical apply remains the funding check. Templates include both offer lanes.
Gossip variants are `DrcOfferCreate` and `DrcOfferCancel`. Blocks that carry
offers use the full block body for IBD. Apply journals offer meta keys, so
reorg and restart restore balances, reserves, receipts, and the state root
together.

RPC:

- `agora_submitDrcOfferCreate`
- `agora_submitDrcOfferCancel`
- `agora_getDrcOffer`
- `agora_getDrcAccountOffers`
- `agora_getDrcBookOffers`

Submit responses include `simulated_fill: false`. `taker_gets_funded` is the
minimum of the remainder and the amount the owner can currently deliver. It is
not a simulated fill. Account cursors are the last offer id. A stale account
cursor is an error. Book cursors are the committed sort key.

## Exclusions

This slice does not implement path payments, autobridging, rippling, transfer
rates, AMM, NFTs, a permissioned DEX, Hooks, or contracts. It does not claim
XRPL transaction encoding, reserve constants, or RPC schemas. Wallet
construction UX is outside this slice. A book walk does not sweep every
expired offer behind the first non-crossing maker. Mempool admission does not
reserve native or issued balances already reserved by payments, escrow, checks,
or channels.
