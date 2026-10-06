# DRC issued asset policy, line freeze, authorization, and clawback

**Maturity:** Experimental · Single-node prototype

Issuer-scoped controls on **issued** `(issuer, currency)` liabilities only. **Native TLT/OVL/DRC** can never be frozen, authorized-by-issuer, or clawed back. Not XRPL wire/API parity.

## rippled 2.5.0 baseline (audit summary)

| rippled amendment / flag | rippled behavior (summary) | Agora bounded slice |
|--------------------------|----------------------------|---------------------|
| `TrustSetAuth` / `lsfRequireAuth` | Issuer must authorize each trust line before holders receive/send (except issuer) | Per-**asset** `require_auth` policy (not account-wide); one-way `AuthorizeHolder` per line |
| `Freeze` / `lsfLowFreeze` / `lsfHighFreeze` / global freeze | Blocks movement of frozen side; global freeze on issuer | Per-asset `global_freeze` + per-line `line_frozen`; blocks **issue** and **holder→holder**; **redeem** still allowed unless deep-frozen |
| `DeepFreeze` | Stricter freeze (no incoming credits) | `line_deep_frozen` requires prior `line_frozen`; blocks **redeem** as well |
| `Clawback` | Issuer pulls issued balance from holder | Exact `DrcIssuedClawbackTx`; requires pre-enabled policy + authorized line; no credit to recipient |
| `NoFreeze` | Issuer renounces freeze | Irreversible `no_freeze` on asset policy |
| Default ripple / transfer rate / paths | — | **Deferred** |

## Deviations from XRPL

- Policy object is keyed by **`IssuedAssetId`**, not issuer account-wide lsf flags.
- Authorization is **explicit per line** (`authorized` on live line v2); under `require_auth`, new lines start **unauthorized** until issuer `AuthorizeHolder`.
- **`require_auth` and `clawback_enabled` enable only at zero outstanding liability.**
- **On `EnableRequireAuth`, every indexed trust line for the asset is atomically persisted as live v2 with `authorized = false`** (no read-time v1 bypass after the policy op commits).
- **`no_freeze` and `clawback_enabled` are irreversible** once set; clearing forbidden.
- **`no_freeze` incompatible** with active `global_freeze`, any line freeze/deep-freeze, or `clawback_enabled`.
- v1 live lines deserialize with deterministic defaults: `authorized = !require_auth`, freezes false (bytes unchanged on disk until rewritten as v2).

## Transfer / redemption gating

| Operation | `require_auth` + unauthorized | `global_freeze` or `line_frozen` | `line_deep_frozen` |
|-----------|------------------------------|----------------------------------|--------------------|
| Issuer→holder issue | reject | reject | reject |
| Holder→holder transfer | reject if either side unauthorized | reject | reject |
| Holder→issuer redeem | reject if sender unauthorized | **allow** | **reject** |

`DepositAuth` / `RequireDestTag` still apply independently.

## Consensus operations (body v17, after v16 trust-line lanes)

1. **`DrcIssuedAssetPolicySetTx`** — issuer signs; DRC fee + nonce/ticket.
2. **`DrcTrustLineIssuerControlTx`** — issuer signs line control (authorize / freeze / deep-freeze).
3. **`DrcIssuedClawbackTx`** — issuer signs exact clawback from one holder line.

Same-block order: **policy → line control → issued transfer / clawback**. Pending mempool dependencies fail closed.

## State

- Asset policy live: `trust/drc/asset-policy/…` (`DrcIssuedAssetPolicyLive`).
- Trust line live **v2**: adds `authorized`, `line_frozen`, `line_deep_frozen`.
- Receipts: policy set, issuer control, clawback (point queries only; no enumeration).
- Reorg journals snapshot both live-state and receipt keys, so orphaned control
  receipts disappear with their policy/line/liability effects.

## Mesh / roots

- Block body **v17** when issued-control lanes non-empty.
- Issued controls entered P2P fingerprint **v21** / state transition **v19**.
  The current aggregate fingerprint is v22 / `agora-trident-state-v20`; it
  preserves the `drc-issued-controls-v1` root semantics.

## Public integration (Experimental)

### RPC submit

- `agora_submitDrcIssuedAssetPolicySet`
- `agora_submitDrcTrustLineIssuerControl`
- `agora_submitDrcIssuedClawback`

### RPC query

- `agora_getDrcIssuedAssetPolicy` — live policy flags for `(issuer, currency)`
- `agora_getDrcIssuedAssetPolicyReceipt` — point receipt by `policy_set_tx_id`
- `agora_getDrcTrustLineIssuerControlReceipt` — point receipt by `control_tx_id`
- `agora_getDrcIssuedClawbackReceipt` — point receipt by `clawback_tx_id`

Admission is fail-closed: issuer DRC nonce, asset-policy slot, trust-line meta (including RequireAuth migration keys), issuer-control slot, and issuer-liability slot must be free. Pending policy blocks issuer control and issued transfer admission on the same asset; pending issuer control blocks clawback on that line.

Gossip: `DrcIssuedAssetPolicySet`, `DrcTrustLineIssuerControl`, `DrcIssuedClawback` (same validation as local admit). v17 bodies with these lanes or multisign attachments use full-block relay (not compact).

## Deferred

Rippling, transfer rates, paths, partial payments, DEX/AMM, LP tokens, credentials, NFTs, XRPL API parity.
