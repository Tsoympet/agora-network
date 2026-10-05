# Native DRC payments

**Maturity:** Experimental.

Trident Phase 4b settles signed DRC payments directly in the canonical L1 DRC
account ledger. Historical district-chain balances, DRC PoW, and bridge-attestor
finality are not part of this path.

## Domain boundary

DRC is a contract-free native payment lane. Its consensus surface may add typed
ledger objects and transitions such as escrow, checks, channels, trust lines,
offers, or AMMs, but it must not accept bytecode, generic call data, deploy
operations, XRPL Hooks, EVM transactions, or user-defined programs. Ordinary
payment conditions do not become contracts merely because funds are held under
explicit release rules. OVL exclusively owns contract and programmable
execution.

## Payment envelope

`DrcPaymentTx` binds the sender, recipient, amount, explicit DRC fee,
destination tag, merchant invoice ID, account nonce, chain ID, and genesis hash
under a secp256k1 signature.

- destination tag `0` means untagged
- invoice ID `Hash::ZERO` means no invoice
- non-zero invoice IDs are unique per recipient merchant
- the nonce is shared with DRC account transfers and DRC stake operations

## Atomic settlement order

Before any write is appended, the transition verifies:

1. version and network-bound authorization
2. non-zero amount and distinct sender/recipient
3. unseen payment ID
4. unused recipient-scoped invoice ID
5. exact account nonce
6. checked `amount + fee`, sender balance, and recipient overflow

Acceptance debits `amount + fee`, credits the recipient amount, sends the fee
to the DRC validator reward pool, records duplicate/invoice indexes, and writes
an immutable `DrcPaymentOutboxEvent`. The outbox is deterministic consensus
metadata for transport consumers; delivery state remains outside consensus.
State-root computation uses a rolling, reorg-journaled payment commitment rather
than rescanning the append-only outbox.

## BlockDAG integration

Payments use `Block.drc_payments`, `agora-block-body-v4`, and
`BlockAcceptanceRecord.payment_statuses`. Account and payment metadata are
journaled for reorg restoration and included in the Trident state root.
`agora_submitDrcPayment` admits signed payments into mempool/gossip/template
flow.

Escrow, recurring authorization, multisig accounts, cross-district paths, and
merchant tag registries remain separate future transitions. Destination tags
are recipient-local routing metadata (as on XRPL), not globally owned names.
DRC is not a stablecoin by virtue of this payment module.

## Pinned XRPL comparison baseline

This roadmap uses **DRC-XRPL baseline 1**, pinned to the protocol vocabulary of
[`rippled` 2.5.0 (2025-06-24)](https://xrpl.org/blog/2025/rippled-2.5.0).
It is a semantic comparison, not wire compatibility, API compatibility, or a
claim that every feature compiled into that release was enabled on XRPL
Mainnet.

The exact amendment corpus for this baseline is:

- `MultiSign`, `ExpandedSignerList`, `Tickets`, and `DisallowIncoming`
- `TrustSetAuth`, `Freeze`, `DeepFreeze`, `Clawback`, and `Flow`
- `CryptoConditions`, `PayChan`, `DepositAuth`, and `Checks`
- `TickSize`, `AMM`, `fixAMMv1_1`, and `AMMClawback`
- `NonFungibleTokensV1_1`, `fixNFTokenRemint`, and
  `fixNonFungibleTokensV1_2`

The canonical amendment descriptions are in the
[XRPL known-amendments registry](https://xrpl.org/resources/known-amendments).
`TokenEscrow`, `Batch`, `PermissionedDEX`, `PermissionDelegation`, `AMMv1_3`,
`EnforceNFTokenTrustlineV2`, and `PayChanCancelAfter` were introduced by 2.5.0
as amendments open for voting, so baseline 1 does not silently assume them.
Multi-purpose tokens and every amendment not listed above are also outside this
baseline.

XRPL accepts both secp256k1 and Ed25519 account keys. Agora permits
**secp256k1 only**, so cryptographic-algorithm parity is explicitly excluded.
No row below changes that constraint.

## Capability matrix

The canonical DRC implementation is **Experimental**. A **Scaffold** entry
means a design target only; historical `agora-bridge-sdk` or
`agora-intent-engine` behavior does not raise canonical L1 maturity.

| Capability | XRPL baseline behavior | Canonical DRC today | Contract-free target / maturity |
| --- | --- | --- | --- |
| Accounts and direct payments | Account balances, sequence numbers, reserves, and `Payment` | Native DRC balance/nonce, signed direct amount+fee settlement, duplicate ID, invoice uniqueness, outbox | Define account reserve/activation policy separately; current direct payment remains **Experimental** |
| Tags and invoice routing | Source and destination tags are routing metadata | Destination tag and recipient-scoped invoice ID; no source tag | Add a versioned source tag and preserve tags in receipts/outbox; **Scaffold** |
| Partial payments | `tfPartialPayment` plus delivered amount and optional `DeliverMin` | No partial delivery; amount settles in full or fails | Add only with explicit delivered-amount semantics and invariant tests; **Scaffold** |
| Paths and pathfinding | Path sets can consume trust lines/order books/AMMs; servers can calculate candidate paths | No canonical path set or pathfinding API | Add deterministic path execution after issued assets and liquidity; keep path search off-consensus with consensus revalidation; **Scaffold** |
| Escrow and payment conditions | Typed time/condition-held value with finish/cancel transitions | No canonical payment escrow | Native typed escrow objects only, with bounded conditions and no scripts/callbacks; **Scaffold** |
| Checks | Create, cash, or cancel a bounded payment authorization | Not implemented | Native check object with expiry, maximum amount, owner reserve, and replay rules; **Scaffold** |
| Payment channels | Fund, claim, and close unidirectional channels | Not implemented | secp256k1 claim authorization, monotonic claims, timeout, and close transitions; **Scaffold** |
| Deposit authorization | Account can require preauthorization for incoming value | Not implemented | Account flag plus typed preauthorization objects; define exemptions for self-funding, escrow, checks, and channels; **Scaffold** |
| Signer lists and multisign | Weighted signer list and quorum-authorized transactions | One network-bound secp256k1 signature | Weighted secp256k1 signer lists with canonical signer ordering and no duplicate keys; **Scaffold** |
| Tickets | Reserved transaction sequence values enable controlled parallel submission | One strict shared account nonce across DRC transfer/payment/stake operations | Native one-use ticket objects integrated with replay and mempool reservation; **Scaffold** |
| Account flags | Master-key, incoming-object, authorization, freeze, and related policy flags | No DRC payment account-policy flags | Versioned, enumerated flags only; unknown/unsafe flags fail closed; **Scaffold** |
| Regular keys | Rotatable secondary key can authorize an account | Address-derived signing key only | secp256k1 regular-key rotation with a no-lockout invariant; **Scaffold** |
| Trust lines and issued assets | Bilateral limits hold issuer-denominated balances | Only protocol-native DRC balances | Issuer-scoped ledger balances are not new protocol-native assets; use typed trust-line state, caps, reserves, and authorization; **Scaffold** |
| Freeze and clawback | Issuer/account controls apply to eligible issued balances | Not implemented; native DRC cannot be issuer-clawed back | Add freeze/deep-freeze/clawback only for opt-in issued assets, never protocol-native DRC; **Scaffold** |
| Rippling and transfer rates | Issuer obligations can route through trust lines under no-ripple and quality rules | Not implemented | Deterministic bounded graph rules after trust lines; default no-ripple for safety; **Scaffold** |
| Order books, offers, and DEX | Native offers cross in deterministic order and can participate in payments | No canonical offers or order book | Typed offer create/cancel/cross transitions, bounded work, deterministic rounding; **Scaffold** |
| AMM | Protocol-native pools integrate with payments and offers | No canonical AMM | Typed constant-product pool transitions only after issued assets/order books; no pool callbacks or programmable curves; **Scaffold** |
| NFTs | Native mint, offers, transfer fees, burn, and amendment fixes | Not implemented | Defer native NFT objects until payment controls and state-growth pricing are stable; never emulate them with DRC contracts; **Scaffold** |
| 2.5.0 payment amendments | Token escrow and payment-channel fixes were proposed in the pinned release | Not implemented and excluded from baseline 1 | Evaluate in a later explicitly pinned baseline; **Scaffold** |
| Hooks, EVM, and contracts | Not part of this bounded XRPL feature corpus | No DRC code/call/deploy envelope exists | Permanently excluded from DRC; OVL is the exclusive execution domain |

## Repository audit evidence

- `DrcPaymentTx` contains payment/routing/authentication fields only; unlike
  `OvlExecutionTx`, it has no gas, bytecode, call-data, or deploy field.
- `Block`, RPC, P2P, mempool, and state-machine admission use distinct
  `drc_payments` and `ovl_executions` lanes.
- `apply_drc_payment` hard-codes `NativeAssetId::DRC`;
  `apply_ovl_execution` hard-codes `NativeAssetId::OVL`.
- The historical bridge SDK has direct/tagged payments and a fixed 1:1
  cross-district `deliver_min` check. It is not trust-line routing, offer
  crossing, partial delivery, or canonical L1 pathfinding.
- The historical intent engine has an in-process constant-product AMM over the
  lab district ledger. It is app-layer orchestration, not a canonical DRC DEX,
  contract VM, or evidence of XRPL parity.
- `revm`, contract create/call data, and `eth_*` compatibility occur only in
  the historical OVL execution stack. No DRC or bridge crate imports `revm`.
- Governance prose such as “milestone contract” describes a grant agreement,
  not executable DRC code. Escrow/bond constants describe held balances, not
  smart contracts.

## Phased implementation plan

Each phase requires a versioned wire/state transition, state-root commitment,
reorg journal, acceptance/mempool/RPC wiring, invariant tests, and an explicit
activation policy. No phase introduces a generic program interpreter.

1. **Payment envelope hardening — Scaffold:** add a versioned source tag,
   delivered-amount receipts, account-policy extension points, and query APIs.
   Do not label fixed full delivery as partial payment.
2. **Account authorization — Scaffold:** regular keys, account flags, signer
   lists/multisig, tickets, and deposit authorization using secp256k1 only.
3. **Conditional payments — Scaffold:** typed escrow, checks, and payment
   channels with expiry, reserve, replay, and close invariants.
4. **Issued-value ledger — Scaffold:** trust lines, authorization, transfer
   rates, no-ripple, freeze/deep-freeze, and opt-in clawback. Native DRC remains
   non-clawbackable.
5. **Liquidity and paths — Scaffold:** offers/order books first, then bounded
   path execution/partial delivery and off-consensus pathfinding, then typed
   AMMs with deterministic rounding.
6. **NFT and later amendment review — Scaffold:** native NFT ledger objects and
   payment interactions only after state-growth fees and authorization rules
   are stable; pin a new XRPL baseline before adopting any 2.5.0-era proposal.

The next bounded slice is phase 1's versioned source-tag support across
`DrcPaymentTx`, its signing bytes, outbox event, TypeScript binding, and focused
settlement/body-root tests. It does not add partial delivery, paths, issued
assets, or execution.
