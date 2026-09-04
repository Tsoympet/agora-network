# Native DRC payments

**Maturity:** Experimental.

Trident Phase 4b settles signed DRC payments directly in the canonical L1 DRC
account ledger. Historical district-chain balances, DRC PoW, and bridge-attestor
finality are not part of this path.

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
