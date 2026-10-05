# Native DRC Payment Channels (typed ledger objects)

**Maturity:** Experimental · Single-node prototype

Payment channels are **typed state-machine objects**, not smart contracts. OVL remains the sole programmable-execution domain.

## Authoritative baseline (rippled 2.5.0)

Rippled [`PaymentChannelCreate`](https://xrpl.org/docs/references/protocol/transactions/types/paymentchannelcreate) locks XRP from the source, names a destination, stores a **33-byte claim public key**, and sets **`SettleDelay`** (seconds) — the minimum time the source must wait before closing a channel that still holds unclaimed funds (claims themselves can settle in the next ledger). Optional immutable **`CancelAfter`** (Ripple Epoch seconds) closes the channel when passed. [`PaymentChannelFund`](https://xrpl.org/docs/references/protocol/transactions/types/paymentchannelfund) adds locked XRP. [`PaymentChannelClaim`](https://xrpl.org/docs/references/protocol/transactions/types/paymentchannelclaim) lets the destination redeem signed cumulative claims, optionally schedule/execute close (`tfClose`), clear mutable `Expiration` (`tfRenew`), or force close when `CancelAfter`/`Expiration` has passed. Off-ledger claim signatures use the channel public key over the channel id and cumulative XRP amount (secp256k1 or Ed25519 on XRPL).

Deposit authorization: third-party **Payment** credits require `DepositPreauth`; destination-driven channel claims are treated like other destination-signed credits in operational docs — Agora pins **destination-submitted on-chain claims** as allowed with `DepositAuth` enabled (same alignment as native DRC checks / destination-signed cash).

## Agora supported subset (explicit deviations)

| Topic | Agora Trident L1 |
| --- | --- |
| Asset | Native **DRC only** |
| Keys | **secp256k1 compressed 33-byte** claim keys only (no Ed25519) |
| Time | **`settle_delay_blue_scores`** and optional **`cancel_after_blue_score`** (GHOSTDAG blue score; **no** wall clock, Ripple Epoch, or mutable `Expiration`) |
| **`tfRenew` / mutable expiration** | **Omitted** — fund may only extend **`cancel_after_blue_score`** forward when explicitly supported |
| Create | Locks **positive** initial amount + owner fee/sequence; caps **32 live channels per owner** |
| Channel ID | `Hash::hash_borsh(DrcPaymentChannelCreateTx)` |
| Off-ledger claim | Domain-separated secp256k1: `chain_id`, `genesis`, `channel_id`, **`channel_version`**, **cumulative authorized amount** |
| On-chain claim | **Destination** submitter; verifies off-ledger sig; **`previous_claimed < cumulative ≤ total_funded`**; pays **destination fee** before delta transfer |
| Close | **Owner** schedules finalize at `blue_score + settle_delay`; **destination** may **finalize immediately**; **any funded submitter** may **finalize** at/after scheduled finalize or **`cancel_after`** (inclusive boundaries documented in code) |
| Invoice | **`invoice_id` must be zero** |
| Destination tag | `Option<u32>`; **`Some(0)` valid**; RequireDestTag enforced at **create** |
| Query | Point lookup only (`channel_id`); **no enumeration** |
| Wire/API | **No XRPL transaction parity** |

## Block lanes (body v15)

After check lanes (v14): `drc_payment_channel_creates`, `drc_payment_channel_funds`, `drc_payment_channel_claims`, `drc_payment_channel_closes`.

## Multisign attachment kinds

`DrcPaymentChannelCreate`, `Fund`, `Claim`, `Close` (appended enum discriminants).

## Mempool

Pending create blocks fund/claim/close for the same `channel_id`. One pending mutating settlement per live channel (claim vs close conflict reservation).
