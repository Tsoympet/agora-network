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

`DrcPaymentTx` has three explicit consensus encodings:

- frozen v1 binds the sender, recipient, amount, explicit DRC fee, destination
  tag, merchant invoice ID, account nonce, chain ID, and genesis hash
- v2 preserves those fields and adds an optional XRPL-style source tag under a
  new signing domain
- v3 preserves those semantics and adds authenticated destination-tag presence
  under a third signing domain

All versions use secp256k1. V1/v2 bytes and signing preimages remain unchanged;
v1 carrying an in-memory source tag fails closed. V2 Borsh encodes
`Option<u32>` deterministically, so no source tag, source tag `0`, and source
tag `u32::MAX` are distinct authenticated values. V3 does the same for the
destination tag.

- destination tag `0` means untagged only in frozen v1/v2
- v3 destination tag `None` means untagged; `Some(0)` is a present tag
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
5. recipient `require_destination_tag` policy
6. exact account nonce
7. checked `amount + fee`, sender balance, and recipient overflow

Acceptance debits `amount + fee`, credits the recipient amount, sends the fee
to the DRC validator reward pool, records duplicate/invoice indexes, and writes
an immutable, payment-versioned `DrcPaymentOutboxEvent` that preserves both
tags. Frozen v1 event bytes remain readable; v2/v3 events use explicit trailing
extensions. It also writes a versioned `DrcPaymentReceipt` under the canonical
payment ID. Receipts have exactly one result, `delivered_exact`, and record
equal authenticated `requested_amount` and `delivered_amount` values. There is
no partial result or delivered-amount input.

The outbox is deterministic consensus metadata for transport consumers; network
message delivery state remains outside consensus. The receipt and outbox event
feed the versioned rolling payment commitment, while the derived payment-ID
lookup key is written in the same atomic batch. Reorg journals restore all of
these keys atomically. Frozen receipt v1 has no variable-length fields (142 bytes
without a source tag, 146 bytes with one), and only one receipt can occupy a
unique payment-ID key; existing block-byte and mempool-count limits bound
admission.

## Recipient-controlled destination-tag policy

`DrcAccountPolicyTx` is a dedicated version-1 operation. It can only set or
clear `require_destination_tag`; it is not a payment, program, Hook, or generic
flag mutation. Its secp256k1 owner signature binds the signing domain, L1 chain
ID, genesis hash, operation version, account, set/clear action, shared DRC
account nonce, and explicit DRC fee. The signer-derived address must equal the
policy account.

Missing policy state is off. When enabled, a payment to that account must carry
an authenticated destination tag. Frozen v1/v2 non-zero tags satisfy the
policy, while their `0` sentinel remains absent. Payment v3 carries
`destination_tag: Some(_)`, and `Some(0)` is valid. Validation happens before
any balance write. The policy applies only to incoming payments, so outgoing
payments are unaffected. Clearing the flag restores untagged receipt.

The canonical same-block lane order is:

```text
account transfers → OVL executions → stake ops → DRC policy ops
  → DRC payments → data commitments
```

Policy operations therefore govern all later payments in the same block.
Earlier account-transfer/stake lanes win shared-nonce conflicts; policy ops win
over later payments from the same owner. A payment invalidated only by ordered
account state is `ConflictLost` under Virtual apply and never mutates balances.
Mempool admission reserves the same DRC account nonce across transfers, stake,
payments, and policy operations. Account-lane replacement is disabled.
A pending, prevalidated set-policy operation evicts already-pending untagged
payments to that recipient and rejects new ones, so local templates cannot
contradict the policy-before-payment lane order. Tagged payments remain
eligible; a clear operation does not authorize untagged relay until it is
canonical.

Accepted policy state, fee reward, account nonce, acceptance result, and reorg
journal commit atomically. Clearing deletes the policy key because absence is
the one canonical false representation. The sorted policy map has a dedicated
root in the composed Trident state root.

## BlockDAG integration

Payments use `Block.drc_payments`; policy operations use the appended
`Block.drc_account_policies` lane. `agora-block-body-v6` wraps the unchanged v5
root with ordered policy-operation IDs, while an empty policy lane retains the
legacy root. Payment IDs remain committed by the existing
`agora-block-body-v4` combiner. `BlockAcceptanceRecord` has aligned payment and
policy statuses. Account, policy, and payment metadata are journaled for reorg
restoration and included in the Trident state root.
`agora_submitDrcPayment` admits signed payments into mempool/gossip/template
flow. Body-root v4 did not need a format change: it commits ordered payment IDs,
and each ID commits the versioned complete signed envelope. Trident protocol
v8, transaction-signing v3, and state-transition
`agora-trident-state-v9` isolate policy/payment-v3 activation from older peers.
This raises the Experimental datadir schema to v12; an older Experimental
datadir must be replayed/reindexed (or recreated). Frozen payment-v1/v2,
outbox-v1/v2, and receipt-v1 bytes are unchanged.

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

`agora_getDrcPaymentByInvoice` is the corresponding exact merchant lookup. It
requires a recipient account plus a 32-byte invoice ID and performs one direct
read of the existing uniqueness key:

```text
payment/drc/invoice/<20-byte recipient><32-byte invoice_id> -> <32-byte payment_id>
```

The derived index is written atomically with the receipt. Its key, value, and
prior absence are captured by the payment reorg journal. The physical index is
not hashed as a separate tree; its complete logical mapping is committed by the
rolling DRC payment root because the receipt and outbox event commit the
recipient, invoice ID, and payment ID. Lookup validates the 32-byte index value,
loads an exact-delivery receipt, and re-checks all three committed fields before
returning it. An inconsistent index fails closed rather than crossing recipient
boundaries.

A valid unknown tuple, invoice ID zero, pending payment, wrong recipient, or
rolled-back payment returns `status: "unknown"`, `payment_id: null`, and
`receipt: null`. There is no prefix scan or invoice enumeration. `settled`
means accepted into the canonical state-machine virtual settlement view; it
does not assert PoW/OVL/DRC checkpoint finality. Both settled query methods
exclude signatures and public keys.

`agora_getDrcAccountPolicy` is a public read-only lookup by account. A known DRC
account returns `status: "known"`, the versioned policy, and the shared account
nonce. An absent account returns `status: "unknown"` with null policy and
nonce. `agora_submitDrcAccountPolicy` submits a fully signed operation and never
accepts private-key material. Malformed accounts or envelopes return JSON-RPC
`-32602`.

Policy reads and exact-delivery receipts describe the canonical state-machine
virtual view. Acceptance/settlement does not assert PoW plus OVL/DRC checkpoint
finality; finality remains independently queryable.

Escrow, recurring authorization, multisig accounts, cross-district paths, and
merchant tag registries remain separate future transitions. Destination tags
are recipient-local routing metadata (as on XRPL), not globally owned names.
Source tags are sender-local routing metadata and are not globally registered.
This bounded feature does not add partial payments, paths, trust lines, issued
currencies, a DEX, memos-as-execution, or contracts. It provides no XRP/XRPL
wire or feature parity, and DRC is not a stablecoin by virtue of this payment
module.

## Next bounded slice

Add recipient-controlled deposit authorization with dedicated, owner-authorized
preauthorization records. It must remain address-based and contract-free, share
the DRC nonce, and reject unauthorized incoming payments before mutation. It
must not add partial payments, pathfinding, trust lines, issued assets, a
DEX/AMM, Hooks, or execution.
