# Native DRC Checks (typed ledger authorizations)

**Maturity:** Experimental · Single-node prototype

## Authoritative baseline (rippled 2.5.0)

Rippled `CheckCreate` records a sender→destination authorization for up to a maximum amount (XRP or issued). Funds are **not** locked at create; the sender must have spendable balance when the check is cashed. Optional expiration uses ledger **close time** (not wall clock in consensus). `CheckCash` is submitted by the destination and may deliver **partial** amounts with `DeliverMin`. `CheckCancel` may be submitted by sender or destination before expiration; **after expiration any account** may cancel.

Rippled deposit authorization ([`depositauth.md`](https://github.com/XRPLF/rippled/blob/release-2.5.0/docs/depositauth.md)): for **CheckCash**, credits sent by the **check destination** are allowed even when `DepositAuth` is enabled; **DepositPreauth is not used for CheckCash** (unlike Payment / EscrowFinish from third parties). Agora matches this: cash submitter must be the destination, so no `DepositPreauth(owner → destination)` gate at cash.

## Agora supported subset (explicit deviations)

| Topic | Agora Trident L1 |
| --- | --- |
| Asset | Native **DRC only** (no issued assets, paths, or conversion) |
| Cash amount | **Exact** only; no cash amount field; no partial cash, `DeliverMin`, or paths |
| Create economics | **No lock/debit** of authorized amount; create charges **submitter fee + sequence only** |
| Expiration | Optional **`expires_after_blue_score`** (GHOSTDAG blue score). Cash allowed while `blue_score < expires`; rejected at `blue_score >= expires` |
| Cancel before expiry | **Owner or destination** only |
| Cancel at/after expiry | **Any funded submitter** (rippled-aligned open cancel) |
| Live cap | **32 live checks per owner** (no XRPL owner reserve object) |
| Destination tag | `Option<u32>` from v1; **`Some(0)` is valid** and satisfies RequireDestTag at create |
| Invoice | **`invoice_id` must be `Hash::ZERO`** (non-zero rejected; no invoice index) |
| DepositAuth on cash | **Rippled-aligned:** destination-signed CheckCash is permitted with `DepositAuth` on; **no** `DepositPreauth` requirement (differs from DRC payment / escrow finish where a third party may submit) |
| Fee on cash | **Cash submitter (destination) pays its own fee** from its balance **before** owner→destination transfer (fee is never taken from cashed proceeds) |
| Query | **Point lookup only** (`check_id`); live query returns `live` or `unknown`; outcomes on **receipt** query |
| Contracts | Checks are **not** smart contracts; OVL remains the only programmable execution domain |

## Check ID

`check_id = Hash::hash_borsh(DrcCheckCreateTx)` (consensus-stable create body hash).

## Block lanes (body v14)

Appended after escrow lanes: `drc_check_creates`, `drc_check_cashes`, `drc_check_cancels` with deterministic ID ordering in `Block::compute_body_root`.

## Mempool (public fail-closed)

- Pending create blocks cash/cancel for the same `check_id`.
- At most one pending cash **or** cancel per live check (settlement reservation).

## Multisign

Detached lane kinds: `DrcCheckCreate`, `DrcCheckCash`, `DrcCheckCancel` (appended enum discriminants).
