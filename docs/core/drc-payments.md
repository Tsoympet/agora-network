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
extension. It also writes a versioned `DrcPaymentReceipt` under the canonical
payment ID. Receipt v1 has exactly one result, `delivered_exact`, and records
equal authenticated `requested_amount` and `delivered_amount` values. There is
no partial result or delivered-amount input.

The outbox is deterministic consensus metadata for transport consumers; network
message delivery state remains outside consensus. The receipt, outbox event,
and their payment-ID indexes feed the versioned rolling payment commitment
rather than requiring a state-root rescan. Reorg journals restore all of these
keys atomically. Receipt v1 has no variable-length fields (142 bytes without a
source tag, 146 bytes with one), and only one receipt can occupy a unique
payment-ID key; existing block-byte and mempool-count limits bound admission.

## BlockDAG integration

Payments use `Block.drc_payments`, the existing `agora-block-body-v4` combiner,
and `BlockAcceptanceRecord.payment_statuses`. Account and payment metadata are
journaled for reorg restoration and included in the Trident state root.
`agora_submitDrcPayment` admits signed payments into mempool/gossip/template
flow. Body-root v4 did not need a format change: it commits ordered payment IDs,
and each ID commits the versioned complete signed envelope. Trident protocol
v7, transaction-signing v2, and state-transition
`agora-trident-state-v8` isolate receipt-state activation from older peers.
Receipt storage raises the Experimental datadir schema to v11; an older
Experimental datadir must be replayed/reindexed (or recreated) to materialize
receipts for payments settled before this activation. Frozen payment-v1 and
outbox-v1 bytes are unchanged.

## Settled payment query

`agora_getDrcPayment` is a public read-only lookup by 32-byte hex
`payment_id`:

- a canonical receipt returns `status: "settled"` and the receipt fields,
  including `result: "delivered_exact"`, both amounts, fee, both tags, routing
  addresses, and invoice ID
- a missing canonical receipt returns `status: "unknown"` and `receipt: null`
- a malformed ID returns the existing JSON-RPC invalid-params error (`-32602`)

The response excludes signatures and public keys. It reports only the
root-committed canonical virtual settlement view. It does not inspect the
process-local mempool, so pending submissions remain `unknown`; it also does
not claim dual-PoS checkpoint finality. Finality remains independently
queryable through the checkpoint RPCs.

Escrow, recurring authorization, multisig accounts, cross-district paths, and
merchant tag registries remain separate future transitions. Destination tags
are recipient-local routing metadata (as on XRPL), not globally owned names.
Source tags are sender-local routing metadata and are not globally registered.
This bounded feature does not add partial payments, paths, trust lines, issued
currencies, a DEX, memos-as-execution, or contracts. It provides no XRP/XRPL
wire or feature parity, and DRC is not a stablecoin by virtue of this payment
module.

## Next bounded slice

Add a recipient-scoped merchant invoice lookup over the existing unique
invoice index. That slice should resolve an invoice to this settled receipt
without adding partial delivery, paths, issued assets, escrow, or execution.
