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

`DrcPaymentTx` has four explicit consensus encodings:

- frozen v1 binds the sender, recipient, amount, explicit DRC fee, destination
  tag, merchant invoice ID, account nonce, chain ID, and genesis hash
- v2 preserves those fields and adds an optional XRPL-style source tag under a
  new signing domain
- v3 preserves those semantics and adds authenticated destination-tag presence
  under a third signing domain
- v4 preserves v3 semantics and adds an optional signed
  `last_valid_blue_score` under a fourth signing domain

All versions use secp256k1. V1–v3 bytes and signing preimages remain unchanged;
v1 carrying an in-memory source tag fails closed. V2 Borsh encodes
`Option<u32>` deterministically, so no source tag, source tag `0`, and source
tag `u32::MAX` are distinct authenticated values. V3 does the same for the
destination tag.

- destination tag `0` means untagged only in frozen v1/v2
- v3 destination tag `None` means untagged; `Some(0)` is a present tag
- source tag `None` means untagged; `Some(0)` is a present source tag
- invoice ID `Hash::ZERO` means no invoice
- non-zero invoice IDs are unique per recipient merchant
- the nonce is shared with DRC account transfers, stake operations, account
  policies, deposit preauthorizations, and payments

## Atomic settlement order

Before any write is appended, the transition verifies:

1. version and network-bound authorization
2. non-zero amount; self-payment follows the rules below
3. unseen payment ID
4. unused recipient-scoped invoice ID
5. recipient `require_destination_tag` policy
6. recipient DepositAuth policy and address preauthorization
7. for payment v4 only, inclusive GHOSTDAG blue-score expiry (see below)
8. exact account nonce
9. checked `amount + fee`, sender balance, and recipient overflow

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

`DrcAccountPolicyTx` is a dedicated owner operation, not a payment, program,
Hook, or generic flag mutation. Frozen envelope v1 can only set or clear
`require_destination_tag`. Envelope v2 can only set or clear
`deposit_auth_required`; cross-version actions fail closed. V1 signing bytes
and persisted policy-v1 bytes are unchanged. A v2 policy state appends the
DepositAuth flag, and clearing that flag canonicalizes to v1 whenever the
remaining destination-tag state is fully representable there.

Both signature versions use secp256k1 and bind their domain, L1 chain ID,
genesis hash, operation version, account, action, shared DRC account nonce, and
explicit DRC fee. V2 additionally binds the explicit operation type. The
signer-derived address must equal the policy account.

Set and clear are idempotent flag assignments: repeating the current value with
the next valid nonce is accepted and still consumes that nonce and fee. A
DepositAuth v2 operation requires an existing canonical DRC owner account.

Missing policy state is off. When enabled, a payment to that account must carry
an authenticated destination tag. Frozen v1/v2 non-zero tags satisfy the
policy, while their `0` sentinel remains absent. Payment v3 carries
`destination_tag: Some(_)`, and `Some(0)` is valid. Validation happens before
any balance write. The policy applies only to incoming payments, so outgoing
payments are unaffected. Clearing the flag restores untagged receipt.

The canonical same-block lane order is:

```text
account transfers → OVL executions → stake ops → DRC regular-key ops
  → DRC signer-list ops → DRC policy ops → DRC deposit-preauthorization ops → DRC payments
  → data commitments
```

Policy operations therefore govern all later payments in the same block.
Earlier account-transfer/stake lanes win shared-nonce conflicts; policy ops win
over later payments from the same owner. A payment invalidated only by ordered
account state is `ConflictLost` under Virtual apply and never mutates balances.
Mempool admission reserves the same DRC account nonce across transfers, stake,
payments, policies, and deposit preauthorizations. Account-lane replacement is
disabled.
A pending, prevalidated set-policy operation evicts already-pending untagged
payments to that recipient and rejects new ones, so local templates cannot
contradict the policy-before-payment lane order. Tagged payments remain
eligible; a clear operation does not authorize untagged relay until it is
canonical.

Accepted policy state, fee reward, account nonce, acceptance result, and reorg
journal commit atomically. Clearing deletes the policy key because absence is
the one canonical false representation. The sorted policy map has a dedicated
root in the composed Trident state root.

## Address-based DepositAuth

This slice implements only recipient-controlled, account-address
preauthorization. Missing `deposit_auth_required` is off. When it is on, an
incoming non-self DRC payment is rejected before mutation unless an active
record exists for exactly `(recipient owner, payment source)`. When it is off,
records are dormant and do not restrict payment; grant records remain stored
until an explicit owner-signed revoke.

`DrcDepositPreauthTx` v1 is a dedicated grant/revoke operation. It is never
encoded as a payment and there is no privileged mutation RPC. Its secp256k1
signature binds:

```text
domain ∥ chain ID ∥ genesis hash ∥ operation type ∥ version
  ∥ owner ∥ authorize/unauthorize ∥ authorized source ∥ shared nonce ∥ fee
```

The signer-derived address must equal `owner`. Owner and source must be
distinct, non-zero, existing canonical DRC accounts. Granting an existing
record, revoking a missing record, wrong-owner signing, tampering, replay,
wrong chain/genesis, future versions, and insufficient fee balance all fail
before writes. A rejected operation charges no fee. Accepted operations consume
the owner's shared DRC nonce and credit their fee to the DRC validator reward
pool atomically with the record and acceptance result.

Policy operations run before preauthorizations, and preauthorizations run
before payments. Consequently, a same-block enable governs all payments; a
grant can authorize a later payment; a revoke blocks a later payment; and a
disable makes records dormant before the payment lane. Earlier account,
execution, or stake operations retain their existing shared-nonce precedence.
Reorg journals restore the account, fee pool, policy, preauthorization record,
payment metadata, and roots. The sorted `(owner, source)` record map has a
dedicated root in the composed state root and persists across RocksDB restart.

Self-payments are authorized without a self-preauthorization, matching the
pinned `rippled` 2.5.0 DepositAuth check. They still require a non-zero amount,
fund `amount + fee`, consume the sender nonce, produce the exact-delivery
receipt/outbox records, and have a fee-only net balance effect. They do not
bypass `require_destination_tag`; if that independent policy is enabled, the
self-payment must carry an authenticated destination tag.

### Pinned XRPL baseline and explicit deviations

The implementation was checked against `rippled` 2.5.0
`DepositPreauth.cpp`, `Payment.cpp`, and `DepositAuthorized.cpp`. The supported
address semantics follow that baseline: owner-controlled grant/revoke,
non-zero and non-self account pairs, existing authorized source, duplicate
grant/missing revoke rejection, self-payment authorization, and dormant records
while DepositAuth is disabled.

This is not XRPL parity:

- credential-based `DepositPreauth` and credential authorization are deferred
- the XRP reserve/unfunded-account DepositAuth exception has no DRC analogue
- XRPL owner-directory, reserve, ledger-entry, account-delete, and `tec`
  fee-result behavior are not reproduced
- Agora rejects invalid operations without charging a fee; it does not emulate
  XRPL transaction-result classes
- no partial payments, paths, checks, escrow, payment channels, trust lines,
  issued currencies, DEX/AMM, NFTs, credentials, Hooks, EVM, or contracts are
  introduced
- DRC remains the native contract-free payment lane; OVL remains the sole
  execution/contracts domain

Mempool admission shares one nonce reservation across all DRC account lanes.
A pending enable or revoke is enforced against later local payment candidates
because those operations precede payments in templates. Pending grants and
disables do not relax canonical admission before settlement. Pending enables
retain payments only when their source has an already-canonical (possibly
dormant) preauthorization; pending revokes evict affected guarded payments.

## Signed GHOSTDAG blue-score expiry (payment v4)

XRPL `LastLedgerSequence` names a monotonic ledger index after which a signed
payment is invalid. Trident maps that role to the **GHOSTDAG blue score of the
containing block** at consensus application time—not wall clock, arrival order,
local virtual-tip guesses, or an unverified header field.

**Determinism.** Each admitted block receives a blue score from the canonical
GHOSTDAG engine (`Ghostdag::add_block_with_work` / persisted `GhostdagRecord`).
The same parent set and block work always yields the same score on every node.
Block construction uses `simulate_blue_score` over the chosen parent set;
settlement uses the score stored for that block when the virtual UTXO/state
transition runs (`apply_block_batched_*_at_blue_score`). Payment v4 therefore
reads `application_blue_score` from the same consensus-derived value that
drives emission and finality metadata.

**Field.** `last_valid_blue_score: Option<u64>` on `DrcPaymentTx` v4 is bound
in the v4 signing preimage. `None` means no expiry (legacy v1–v3 behavior).
`Some(cutoff)` uses **inclusive** semantics: the payment is valid when
`application_blue_score <= cutoff` and rejected when
`application_blue_score > cutoff`. Tampering the cutoff invalidates the
signature; changing any bound field changes `payment_id` and the rolling payment
root.

**Atomic rejection.** Expiry is checked after auth/version/policy/preauth gates
and before nonce/balance writes. A rejected expiry does not debit balances,
consume nonce, charge fee, write receipt/outbox/invoice/seen keys, or advance the
payment root.

**Mempool and templates (non-authoritative).** Admission and template selection
revalidate expiry against the current virtual blue score and
`next_template_blue_score` respectively. Expired entries are rejected or omitted
and evicted with nonce reservation released. After a reorg lowers the virtual
score, a payment that was locally expired may become admissible again if still
within its signed cutoff—consensus application remains authoritative.

Receipt v3 and outbox events for v4 payments persist `last_valid_blue_score`
when present. Settled RPC queries include the cutoff only when the canonical
receipt carries it; there is no durable “expired” status for unsettled payments.

## BlockDAG integration

Payments use `Block.drc_payments`; policy operations use
`Block.drc_account_policies`; grants/revokes use the appended
`Block.drc_deposit_preauths` lane. `agora-block-body-v7` wraps the unchanged v6 root with ordered
preauthorization-operation IDs. `agora-block-body-v8` further wraps v7 when any
payment envelope is v4 (ordered payment IDs over the v7 root). V6 continues to
wrap ordered policy-operation IDs. Payment IDs remain committed by the existing
`agora-block-body-v4` combiner. `BlockAcceptanceRecord` has aligned policy,
preauthorization, and payment statuses. Account, policy, preauthorization, and
payment metadata are journaled for reorg restoration and included in the
Trident state root.
`agora_submitDrcPayment` admits signed payments into mempool/gossip/template
flow. Trident protocol v10, transaction-signing v5, state-transition
`agora-trident-state-v11`, state-root v8, and body-root v8 isolate payment-v4
expiry from older peers. This raises the Experimental datadir schema to v14; an
older Experimental datadir must be replayed/reindexed (or recreated). Frozen
payment-v1/v2/v3, policy-v1, outbox-v1/v2/v3, receipt-v1/v2, body-v1–v7, and
historical acceptance/journal bytes remain readable and unchanged.

## Settled payment query

`agora_getDrcPayment` is a public read-only lookup by 32-byte hex
`payment_id`:

- a canonical receipt returns `status: "settled"` and the receipt fields,
  including `result: "delivered_exact"`, both amounts, fee, both tags, routing
  addresses, invoice ID, and `last_valid_blue_score` when the settled payment
  was v4 with a signed cutoff
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

`agora_getDrcDepositPreauth` is an exact public point query by recipient
`owner` and `authorized_source`. When both canonical DRC accounts exist, it
returns `status: "known"` plus `preauthorized`, the owner's
`deposit_auth_required`, and the effective `deposit_authorized` result. The
effective result is true for self, while DepositAuth is disabled, or when the
exact record exists. If either account is unknown, all three fields are null
and the status is `unknown`. There is no prefix scan, enumeration, signature,
public key, or private material. `agora_submitDrcDepositPreauth` accepts only a
fully signed public envelope. Malformed pairs or envelopes return `-32602`.

Policy and preauthorization reads and exact-delivery receipts describe the
canonical state-machine virtual view. Acceptance/settlement does not assert PoW
plus OVL/DRC checkpoint finality; finality remains independently queryable.

Escrow, recurring authorization, cross-district paths, and
merchant tag registries remain separate future transitions. Destination tags
are recipient-local routing metadata (as on XRPL), not globally owned names.
Source tags are sender-local routing metadata and are not globally registered.
This bounded feature does not add partial payments, paths, checks, escrow,
channels, trust lines, issued currencies, a DEX/AMM, NFTs, credentials,
memos-as-execution, Hooks/EVM, or contracts. It provides no XRP/XRPL wire or
feature parity, and DRC is not a stablecoin by virtue of this payment module.

## secp256k1 regular keys (bounded)

`DrcRegularKeyTx` v1 is a dedicated set/clear operation for one rotatable
secondary key per DRC account. It is never a payment and has no privileged RPC
mutation. Signatures bind domain, chain ID, genesis hash, operation type,
version, owner, action, canonical regular-key identity (33-byte compressed
secp256k1 public key and derived address for `set`; empty for `clear`), shared
nonce, and fee.

The master-derived address is always authorized. The installed regular key may
also authorize every in-scope DRC account operation: DRC account transfers, DRC
stake operations, policy, deposit preauthorization, payments, and further
regular-key rotation/clear. OVL lanes remain master-only in this slice.

Pinned against `rippled` 2.5.0 `SetRegularKey` / `RegularKey` semantics with
explicit deviations:

- secp256k1 only; no Ed25519 or XRPL wire compatibility
- master-key disable remains excluded (no-lockout invariant)
- master cannot be installed as its own regular key; zero keys are rejected
- the current regular key may rotate or clear itself; the master can always recover
- Agora uses address-derived identity plus an explicit 33-byte pubkey binding on set
- rejected operations charge no fee; there is no XRPL `tec` class emulation

## Weighted secp256k1 signer lists and multisign (bounded)

`DrcSignerListTx` v1 installs, replaces, or deletes one canonical ordered list per
DRC account: `(signer address, weight)` entries sorted by signer, plus a non-zero
quorum. At most **32** entries (`rippled` 2.5.0 `ExpandedSignerList` cap; not XRPL
wire parity). Weights are `u16` (max 65535). The owner cannot appear on its own list.
Empty lists, zero weights/quorum, duplicate signers, impossible quorum, and owner-as-signer
fail before mutation.

Pinned `rippled` 2.5.0 `SignerListSet` / `MultiSign` baseline with explicit deviations:

- secp256k1 only; no Ed25519, ticket/credential paths, or XRPL result classes
- one bounded list per account; no master-key disable in this slice
- master and regular-key single signatures remain valid recovery paths
- multisign uses domain `agora-trident-drc-multisign-participant-v1` binding
  `signing_for`, chain ID, genesis, operation signing bytes, and version
- consensus bodies carry detached authorization in `drc_multisign_attachments`
  (Borsh lane, body-root v11); operation vec elements stay byte-stable (no inline
  multisign in lane Borsh). Mempool/RPC JSON may hold `multisign` until template
  materialization strips it into attachments keyed by
  `(DrcMultisignOperationKind, signing_commitment)` where
  `signing_commitment = Hash(domain, key_version, kind, signing_bytes_bound(...))`
- attachments are strictly sorted by `(kind, signing_commitment)`, capped at one
  per eligible in-scope DRC operation (≤32 auth entries per bundle); orphans,
  duplicates, mixed inline+attachment, and wrong-kind/owner keys fail before mutation
- exactly one of single-signature or multisign authorization; mixed envelopes fail closed
- signer-list **Set** may be authorized by master, regular key, or multisign on the
  **currently installed** list; signers that exist only on the **new** list cannot
  authorize their own installation
- extra valid listed signatures beyond quorum are allowed up to 32, processed in strict
  signer order until quorum is met

Multisign is an alternate authorization path for every DRC account operation already
covered by regular-key auth: account transfers, stake ops, policy, deposit preauth,
payments, regular-key rotation, and signer-list operations. OVL lanes remain master-only.

Canonical same-block order:

```text
account transfers → OVL executions → stake ops → DRC regular-key ops
  → DRC signer-list ops → DRC policy ops → DRC deposit-preauthorization ops → DRC payments
  → data commitments
```

Signer-list state commits to `agora-drc-signer-list-root-v1` inside composed state root
`agora-trident-state-root-v10`. Body commitment uses `agora-block-body-v10` when the
signer-list lane is non-empty and `agora-block-body-v11` when
`drc_multisign_attachments` is non-empty (attachment leaf IDs commit full auth bytes).
Trident protocol v13, state transition `agora-trident-state-v14`, transaction signing
`agora-trident-tx-v7`, and Experimental datadir schema v17 isolate this slice.
Multisign is usable on mined blocks once template materialization and P2P full-body
transport carry the attachment lane (Single-node prototype).

`agora_submitDrcSignerList` admits a fully signed operation.
`agora_getDrcAccountSignerList` returns `quorum`, `entry_count`, and `account_nonce`
for a known account without enumerating signer identities. Malformed inputs return
`-32602`.

### DRC master-key disable (rippled 2.5.0 subset, explicit deviations)

Pinned baseline: rippled 2.5.0 `asfDisableMaster` / `lsfDisableMaster` intent — the
owner master secp256k1 key stops authorizing DRC account operations while alternate
recovery remains. **Not XRPL wire/API parity** (no `lsf`, Tickets, credentials, or
`tec`-class semantics).

Supported Agora subset:

- DRC-scoped only; OVL/TLT authorization unchanged
- policy v3 tx actions `set_master_key_disabled` / `clear_master_key_disabled` on the
  existing signed `DrcAccountPolicyTx` lane (no privileged RPC)
- **Enable:** owner **master key only** (no regular key, no multisign); requires a
  live regular key **or** signer list in canonical state before mutation
- **While disabled:** master single-signatures fail in the central DRC verifier for
  transfers, stake, regular-key, signer-list, policy, DepositPreauth, and payments;
  regular-key and signer-list multisign paths remain valid
- **Clear:** valid regular key or current signer-list multisign; disabled master
  cannot clear itself; master authorization resumes immediately after clear
- **No-lockout:** while disabled, regular-key clear/replace and signer-list
  delete/replace cannot remove the last alternate recovery path; same-block lane
  order uses the copy-on-write overlay after each canonical mutation

Body/state/protocol bumps: policy state v3 (`master_key_disabled`), tx/signing v3 domain
for disable actions, Trident protocol **v14**, state transition **`agora-trident-state-v15`**,
transaction signing **`agora-trident-tx-v8`**, datadir schema **18**. Maturity:
Single-node prototype (consensus + RPC query; not XRPL parity).

`agora_getDrcAccountPolicy` exposes `master_key_disabled` alongside existing flags.

**Mempool / mining templates (bounded, fail-closed):** account-lane admission reserves the
owner shared nonce but does **not** simulate same-block recovery mutations (regular-key,
signer-list, enable/clear) against a prospective overlay. Template builders must apply
candidate blocks against canonical state (or an explicit copy-on-write overlay identical to
consensus lane order). A locally queued `set_master_key_disabled` is therefore **not**
safe to pair in one template with recovery ops that only exist in sibling pool entries at
the same nonce — block apply remains authoritative, and invalid pairings fail closed at
apply rather than producing a body that would lock the account.

Regular-key state commits to `agora-drc-regular-key-root-v1` inside composed
state root `agora-trident-state-root-v9`. Body commitment uses
`agora-block-body-v9` when the lane is non-empty. Trident protocol v11,
state transition `agora-trident-state-v12`, and Experimental datadir schema v15
isolate the regular-key slice. Frozen payment/policy/preauth encodings remain readable.

`agora_submitDrcRegularKey` admits a fully signed operation.
`agora_getDrcAccountKeys` returns `regular_key` and `account_nonce` for a known
DRC account, or `unknown` when absent. Malformed inputs return `-32602`.

## Next bounded slice

Master-key disable with signer-list-only recovery, credential-based `DepositPreauth`,
recurring pull payments, and cross-asset routing remain out of scope for the
native DRC payment lane.
