# Native DRC payments

**Maturity:** Experimental.

Trident Phase 4b settles signed DRC payments directly in the canonical L1 DRC
account ledger. Historical district-chain balances, DRC PoW, and bridge-attestor
finality are not part of this path.

## Domain boundary

DRC is a contract-free native payment lane. Source and destination tags are
typed routing metadata, not executable input. DRC accepts no bytecode, generic
call data, deploy operation, Hook, EVM transaction, or user-defined program.
OVL remains the only Trident execution and contracts domain.

## Payment envelope

`DrcPaymentTx` has two explicit consensus encodings:

- frozen v1 binds the sender, recipient, amount, explicit DRC fee, destination
  tag, merchant invoice ID, account nonce, chain ID, and genesis hash
- v2 preserves those fields and adds an optional XRPL-style source tag under a
  new signing domain

Both versions use secp256k1. V1 bytes and signing preimages remain unchanged;
v1 carrying an in-memory source tag fails closed. V2 Borsh encodes
`Option<u32>` deterministically, so no source tag, source tag `0`, and source
tag `u32::MAX` are distinct authenticated values.

- destination tag `0` means untagged
- source tag `None` means untagged; `Some(0)` is a present source tag
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
an immutable, payment-versioned `DrcPaymentOutboxEvent` that preserves both
tags. Frozen v1 event bytes remain readable; v2 events use an explicit trailing
extension. The outbox is deterministic consensus metadata for transport
consumers; delivery state remains outside consensus. State-root computation
uses a rolling, reorg-journaled payment commitment rather than rescanning the
append-only outbox.

## BlockDAG integration

Payments use `Block.drc_payments`, the existing `agora-block-body-v4` combiner,
and `BlockAcceptanceRecord.payment_statuses`. Account and payment metadata are
journaled for reorg restoration and included in the Trident state root.
`agora_submitDrcPayment` admits signed payments into mempool/gossip/template
flow. Body-root v4 did not need a format change: it commits ordered payment IDs,
and each ID commits the versioned complete signed envelope. Trident protocol
v6, transaction-signing v2, and state-transition
`agora-trident-state-v7` isolate this activation from older peers.

Escrow, recurring authorization, multisig accounts, cross-district paths, and
merchant tag registries remain separate future transitions. Destination tags
are recipient-local routing metadata (as on XRPL), not globally owned names.
Source tags are sender-local routing metadata and are not globally registered.
This bounded feature does not add partial payments, paths, trust lines, issued
currencies, a DEX, memos-as-execution, or contracts. It provides no XRP/XRPL
wire or feature parity, and DRC is not a stablecoin by virtue of this payment
module.

## Next bounded slice

Add a delivered-amount receipt and payment query API for the existing fixed
full-delivery transition. That slice must not introduce partial delivery,
paths, issued assets, or execution.
