# DRC issuer-scoped trust lines and issued-value transfers

**Maturity:** Experimental · Single-node prototype

Contract-free issuer liabilities in the DRC payment domain. Not native DRC, not XRPL/XRP parity, and not a native stablecoin facility.

## rippled 2.5.0 subset and Agora deviations

| rippled concept | Agora slice (this phase) | Excluded |
|-----------------|--------------------------|----------|
| `TrustSet` (bilateral limit/balance) | Holder-signed `DrcTrustLineSetTx`: positive limit creates/updates; limit zero deletes only at zero balance | Rippling, transfer rate/quality, default ripple |
| Issued currency `Amount` | Fixed-width `IssuedCurrencyCode` + `IssuedAmount` (u64 units); `IssuedAssetId { issuer, currency }` | Locale/string normalization in consensus; IOU paths |
| `Payment` (issued) | Exact `DrcIssuedTransferTx`: issuer→holder issue, holder→issuer redeem, holder→holder transfer | Partial payments, paths, conversion, offers/DEX/AMM |
| Checks/escrow/channels for IOU | — | All issued-value use of those lanes |
| Reserve | Hard caps: 64 lines/holder, 256 holders/issuer/currency index | XRPL reserve math |

## Consensus operations

- **TrustLineSet** (body v16 lane `drc_trust_line_sets`): holder signs; DRC fee + nonce/ticket; master/regular/multisign unchanged from other DRC ops.
- **IssuedTransfer** (body v16 lane `drc_issued_transfers`): sender signs; positive `IssuedAmount`; DRC fee separate; `invoice_id` must be zero; optional source/destination tags persisted.

## Policy

- `RequireDestTag` on recipient: missing tag rejects; `Some(0)` allowed.
- `DepositAuth`: original sender→recipient; preauth via existing deposit-preauth meta.
- Policy failure leaves issued meta unchanged.

## State

- Live line: `DrcTrustLineLive` in Meta (`trust/drc/line/…`).
- Issuer outstanding: `DrcIssuerLiability` (`trust/drc/liability/…`).
- Transfer receipt: `DrcIssuedTransferReceipt` v2 (`trust/drc/xfer/…`; includes source/destination tags).
- State root: `drc-check-paychan-v2` commits check, payment-channel, and `drc-issued-liability-v1` trust-line root.

## Block / mesh versions

- Body: `TRIDENT_BLOCK_BODY_V16` when trust-line lanes non-empty.
- P2P fingerprint: current aggregate `TRIDENT_PROTOCOL_VERSION` 24; state
  transition `agora-trident-state-v22`.
- Issued controls (policy/freeze/clawback): see [`drc-issued-controls.md`](drc-issued-controls.md).
- Gossip: `NetworkMessage::DrcTrustLineSet`, `NetworkMessage::DrcIssuedTransfer` on the transactions topic.
- Live holder lines are common schema-21 ledger objects owned by the holder.
  Issuer liabilities remain aggregate accounting state, not owner-directory
  objects.

## Public RPC (admission, not finality)

| Method | Role |
|--------|------|
| `agora_submitDrcTrustLineSet` | Mempool admit after virtual apply; reserves holder nonce/ticket and line slot |
| `agora_submitDrcIssuedTransfer` | Mempool admit after virtual apply; reserves sender nonce/ticket, line/liability overlay |
| `agora_getDrcTrustLine` | Point query: `live` returns limit/balance; `unknown` otherwise |
| `agora_getDrcIssuerLiability` | Point query: outstanding units for `(issuer, currency)` |
| `agora_getDrcIssuedTransferReceipt` | `known` receipt or `unknown` |

**Currency input:** exactly 40 hex digits (20 bytes) or uppercase 3-character standard code only. No lowercase standard codes, locale aliases, or string normalization.

**Mempool reservations (public):** one pending `TrustLineSet` per `(holder, asset)`; pending create/delete blocks dependent transfers; pending issue/redeem serializes issuer liability mutation; pending balance deltas enforce recipient limits and sender balances; canonical meta-key reservations (`trust/drc/line/…`, `trust/drc/liability/…`) are inspectable in tests; release on block inclusion, eviction, and reorg. After canonical state changes, pending issued transfers are revalidated and dropped when overlay fails.

**Reorg:** explicit resubmit of evicted mempool txs; duplicate resubmit rejected; chain tip reorg restores line, liability, control/transfer receipts, and native fee state.

**`agora_getDrcTrustLine` schema:** `status` (`live`|`unknown`), `limit`, `balance` (issued units as decimal strings), `holder`, `issuer`, `currency` — balance is not a separate RPC method.

## Deferred

Rippling, transfer rates, paths, partial pay, DEX/AMM, issued-value checks/escrow/channels, credentials, NFTs, and XRPL wire/API parity. Issuer authorization, freeze/deep-freeze, and clawback are the bounded controls documented in [`drc-issued-controls.md`](drc-issued-controls.md), not full XRPL parity.
