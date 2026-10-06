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
| **`tfRenew` / mutable expiration** | **Omitted** — `PaymentChannelFund` adds locked XRP only; **`CancelAfter` is immutable** at create |
| Create | Locks **positive** initial amount + owner fee/sequence; caps **32 live channels per owner** |
| Channel ID | `Hash::hash_borsh(DrcPaymentChannelCreateTx)` |
| Off-ledger claim | Domain-separated secp256k1: `OFFLEDGER_CLAIM_DOMAIN`, `chain_id`, `genesis`, **`channel_id`**, **cumulative authorized amount** (no mutable channel version; funding does not invalidate prior signatures) |
| On-chain claim | **Destination** submitter; verifies off-ledger sig; **`previous_claimed < cumulative ≤ total_funded`**; pays **destination fee** before delta transfer; **rejected at `blue_score >= cancel_after`** (strictly before cutoff only) |
| Close | **Owner** `OwnerScheduleClose` sets finalize at `blue_score + settle_delay`; **destination** `DestinationClose` finalizes immediately; **owner or destination** `Finalize` at/after scheduled finalize or **`cancel_after`** (inclusive). **Bounded deviation:** rippled allows additional close flag combinations; Agora pins the three explicit `close_kind` values only. |
| Invoice | **`invoice_id` must be zero** |
| Destination tag | `Option<u32>`; **`Some(0)` valid**; RequireDestTag enforced at **create** |
| Query | Point lookup by `channel_id`; common bounded owner-object pagination for live channels |
| Wire/API | **No XRPL transaction parity** |

## Block lanes (body v15)

After check lanes (v14): `drc_payment_channel_creates`, `drc_payment_channel_funds`, `drc_payment_channel_claims`, `drc_payment_channel_closes`.

## Multisign attachment kinds

`DrcPaymentChannelCreate`, `Fund`, `Claim`, `Close` (appended enum discriminants).

## rippled 2.5.0 pinned behavior matrix (containing-block blue score)

| Scenario | Agora rule (fail-closed) |
| --- | --- |
| Fund after owner schedule close | **Allowed** — adds locked DRC; does not mutate claim key, destination, or `cancel_after_blue_score`; writes immutable fund event |
| Claim during owner settle delay | **Allowed** — destination-only; off-ledger sig required; cumulative strictly increases |
| Claim at/after `CancelAfter` | **On-chain claim rejected** at `blue_score >= cancel_after`; **finalize** still allowed at inclusive `blue_score >= cancel_after` |
| Repeat owner schedule | Each `OwnerScheduleClose` sets `close_finalizable_after = score + settle_delay` (resets delay from containing block) |
| Destination immediate close | `DestinationClose` finalizes in same block; remainder returns owner once |
| Who may `Finalize` | Owner or destination when `payment_channel_finalize_allowed` (inclusive `>=` scheduled finalize or cancel cutoff) |
| Create `CancelAfter` | Strictly **future** vs create block score; bounded `<= MAX_BLUE_SCORE_BOUND` |
| Create settle delay | Non-zero; `score + delay` must not overflow at create apply |
| RequireDestTag | Enforced at create; `Some(0)` satisfies policy |
| DepositAuth | Does **not** block destination-submitted claims |


Pending create blocks fund/claim/close for the same `channel_id`. One pending mutating settlement per live channel (claim vs close conflict reservation).

## Public network integration (Experimental)

- **P2P protocol fingerprint:** payment channels entered at v19 (body v15);
  the current aggregate fingerprint is v23 after trust-line, issued-control,
  DRC fee-burn accounting, and the common ledger-object index.
- **Mempool / RPC admission:** dry-run virtual apply, nonce/Ticket reservation, fail-closed while create pending, one pending mutator per live channel; reservations released on reject, eviction, and block inclusion.
- **Reorg / resubmission:** virtual reorg restores canonical channel, ticket, and account snapshots from journals; included txs are **not** auto-reinserted into the mempool — operators must **explicitly resubmit** once canonical state allows (duplicate resubmit remains fail-closed).
- **Dependency policy:** pending create → fund/claim/close rejected publicly; duplicate channel mutation rejected; malformed off-ledger claims never templated.
- **DepositAuth:** does **not** gate destination-submitted on-chain claims (third-party credits remain policy-gated separately).
- **Queries / verify:** point lookups, common owner-object pagination, and `agora_verifyDrcPaymentChannelClaim` read canonical virtual-view state only; **`accepted` / `live` / `known` do not assert dual-PoS checkpoint finality**.
- **Exclusions:** no XRPL transaction/API parity and no invoice routing (`invoice_id` must be zero).

See [`rpc.md`](rpc.md) for JSON-RPC shapes. Maturity remains **Experimental · Single-node prototype** until multi-node devnet hardening completes.
