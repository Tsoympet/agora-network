# Native DRC escrow (typed, contract-free, blue-score time gates)

**Maturity:** Multi-node devnet (bounded Trident slice; not XRPL parity; not smart contracts).

## rippled 2.5.0 Escrow audit (supported Agora subset)

| rippled Escrow concept | Agora Trident |
| --- | --- |
| EscrowCreate locks XRP from owner | **Yes:** locks native **DRC** from owner into a typed escrow object |
| FinishAfter / CancelAfter time fields | **Yes:** `finish_after_blue_score` / `cancel_after_blue_score` only (GHOSTDAG blue score, never wall clock) |
| Third-party EscrowFinish | **Yes:** any funded DRC account may submit finish and pay its own fee/sequence |
| Third-party EscrowCancel | **Yes (rippled 2.5.0):** any funded DRC account may submit cancel after `CancelAfter`; submitter pays fee/sequence; locked DRC returns to escrow **owner** only ([XRPL EscrowCancel](https://xrpl.org/docs/references/protocol/transactions/types/escrowcancel)) |
| CryptoConditions / hashlocks | **Excluded** |
| Issued assets / trust lines | **Excluded** (DRC native only) |
| Hooks / callbacks / predicates | **Excluded** |
| Public XRPL wire/API parity | **Excluded** |

## Pinned cutoff semantics (inclusive/exclusive)

Let `S` be the containing block’s canonical GHOSTDAG blue score.

- **Create:** at least one of `finish_after_blue_score` or `cancel_after_blue_score` must be present. If both are present, require `finish_after < cancel_after`. Bounds must be strictly below `u64::MAX`.
- **Finish** allowed iff:
  - live escrow exists and is unsettled;
  - if `finish_after` is `Some(F)`: `S >= F`;
  - if `cancel_after` is `Some(C)`: `S < C` (cancel window is exclusive for finish; the cancel score itself belongs to cancel only).
- **Cancel** allowed iff:
  - live escrow exists and is unsettled;
  - `cancel_after` is `Some(C)` and `S >= C`;
  - submitter is any funded DRC account authorized like other DRC ops (master/regular/multisign + master-disable rules on **submitter**, not owner).

If only `cancel_after` is set (no finish_after), finish is allowed for all `S < C`.

## Policy interaction (DepositAuth / RequireDestTag)

- **Create:** if recipient `RequireDestTag` is active, create must carry an authenticated destination tag (payment v3 semantics). DepositAuth is **not** evaluated at create (funds remain with owner-side lock).
- **Finish:** recipient DepositAuth is evaluated with **escrow owner** as deposit source against canonical preauthorization state at finish time. Policy failure rejects finish and leaves escrow/value untouched.
- **Cancel:** no DepositAuth path (return to owner).

## Object model and caps

- **Escrow ID:** `Hash::hash_borsh(DrcEscrowCreateTx)` (collision-resistant, non-circular).
- **Live cap:** at most **32** live escrows per owner (no XRPL reserve model).
- **Supply:** locked amounts leave spendable owner balance but remain supply-accounted via `drc_escrow_root` until finish or cancel.
- **Receipts:** immutable `DrcEscrowReceipt` records exact finish/cancel delivery with routing metadata.

## Block lanes (body v13)

After ticket creates, consensus applies in order:

1. `drc_escrow_creates`
2. `drc_escrow_finishes`
3. `drc_escrow_cancels`

Same-block create→finish/cancel is allowed deterministically via the overlay (mempool remains fail-closed for pending create without inclusion).

## RPC

- Submit: `agora_submitDrcEscrowCreate`, `agora_submitDrcEscrowFinish`, `agora_submitDrcEscrowCancel`
- Point query: `agora_getDrcEscrow` → `live` | `unknown`; `agora_getDrcEscrowReceipt` → settled outcome or absent

No enumeration RPC.
