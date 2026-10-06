# Trident data-commitment consensus lane

**Maturity:** Experimental. The consensus block/state lane, standalone gossip,
mempool, template selection, submit/get RPC, and TLT inclusion fee exist.
Default Experimental boot binds the DA network fingerprint to the live mesh and
burns `DA_INCLUSION_FEE_TLT` (0.01 TLT) from the operator's P2PKH UTXOs on
accept. Public district endpoints do not.

## Roadmap interpretation

The unchecked roadmap text is:

> **4.6 (Ops):** Post DA commitments from `agora-layers` into live L1 txs / public district endpoints

It entered the historical layered Phase 4 in commit `26ef4a9` on 2026-08-03,
before the Trident design freeze dated 2026-08-06. Its implied L2/L3 monetary
architecture is superseded. `agora-layers` is now a single-process lab and
reuse stack; OVL and DRC are protocol-native Trident L1 assets. A lab balance,
mint, credit, timer status, or district record is never canonical L1 state.

The useful part of the old item is narrower: Trident may eventually include a
versioned transaction that commits integrity/provenance for explicitly
experimental lab data. Such a commitment does not migrate value, prove data
availability by itself, or satisfy Trident finality.

## Audit result

| Area | Current behavior | Consequence |
| --- | --- | --- |
| Lab commitment | `BatchCommitment` has batch/state/transaction roots, but its legacy bytes have no source or L1 domain | Not safe to submit as a memo or generic blob |
| Lab `recordDa` | Stores an in-process operator assertion only | Not evidence of L1 submission, acceptance, confirmation, or finality |
| Lab timer status | `finalizeDue` advances on the historical challenge timer without consulting L1 | Must never be called Trident finality |
| L1 body | `Block.data_commitments` is appended after the v4 lanes and committed by body-root v5 | Existing UTXO/v2/v3/v4 roots remain unchanged when the DA lane is empty |
| L1 state | Accepted authorizations, per-operator replay nonces, `(source, sequence)` indexes, acceptance status, state root, and revert snapshots are atomic | Exact signed retries are idempotent; conflicts follow Virtual blue order and reorg cleanly |
| P2P/mempool/template | `NetworkMessage::DataCommitment` on the tx topic; mempool reserves `(source, sequence)` and operator nonce; templates include the lane when the DA fingerprint is set (default Experimental boot) | Operator must hold ≥ `DA_INCLUSION_FEE_TLT` |
| RPC | `agora_submitDataCommitment` (token-gated) and public `agora_getDataCommitment` with pending/accepted/confirmed/finalized/conflict_lost/reverted | Submit rejects without fingerprint or without TLT funds; get never maps lab `recordDa` to finality |
| Layer checkpoint | Persists ledger/runtime snapshots, but not tracked batch commitments or local DA flags | A restarted process cannot reconstruct old submit intent; there is no durable L1 outbox, retry state, tx id, or confirmation state |
| District HTTP surface | Mixed reads with mint/credit/payment mutations, no bearer auth, no rate limiter | It is forced to loopback and is not a public district API |
| Trident boot | Genesis v3 live-state materialization and startup gate remain incomplete | No public Trident network target is available |

The existing OVL execution `data` field is not a substitute: non-empty contract
data is intentionally rejected, and using it as an undocumented memo would
evade a reviewed transaction kind, fee policy, acceptance rules, and state-root
commitment.

## Authenticated payload (PR #120)

`agora-types` defines:

- `DataAvailabilityCommitment`: versioned, domain-separated Borsh fields for
  the lab source chain/genesis, batch id/sequence, pre/post state roots,
  transaction Merkle root/count, and source timestamp;
- `DataCommitmentSource::AgoraLayersOvolosBatchLab`: the non-canonical source
  label is part of the committed bytes;
- `DataCommitmentAuthorization`: operator, replay nonce, commitment, compressed
  secp256k1 public key, and compact signature.

The authorization preimage binds:

```text
authorization domain
  || L1 chain id
  || L1 genesis/identity hash
  || L1 network fingerprint
  || authorization version
  || operator address
  || replay nonce
  || domain-separated source commitment id
```

`agora-crypto` signs and verifies this preimage through the existing audited
`secp256k1` wrapper. No key handling or cryptographic primitive is implemented
in the lab runtime. `LayersRuntime::l1_da_commitment_candidate` performs only a
deterministic, read-only conversion from a known lab batch and validates its
source provenance.

## Consensus consumer

`Block.data_commitments` is an appended Borsh field. Body-root v5 wraps the
unchanged v4 root and the ordered authorization IDs; empty DA lanes retain the
legacy root. Block decoding accepts an exact end-of-input at each historical
appended-lane boundary while rejecting partial/truncated lane lengths.

Virtual apply verifies every commitment's structure, compressed secp256k1 key,
signature, L1 chain/genesis, network fingerprint, and replay nonce before a
soft status is possible. The first authorization in blue order to claim a
`(DataCommitmentSource, sequence)` key is `Accepted`; the exact same signed
authorization is `ExactDuplicate`; a different authorization or consumed
operator nonce is `ConflictLost`. Invalid authentication fails the block.

Accepted records and operator replay cursors live under `da/v1/…` Meta keys.
They entered the composed root at v7 and remain committed by the current
`agora-trident-state-root-v16`. Their prior values are stored in `UtxoJournal`
and apply/revert with acceptance in the same `WriteBatch`, so reorg and crash
recovery use the existing `pending_virtual` protocol.

The DA lane entered at Trident protocol v10 / state transition v11. Protocol
v25 appends standalone `NetworkMessage::DataCommitment` gossip. The current
aggregate fingerprint is protocol v27 / `agora-trident-state-v22` after typed
compact gossip. Frozen pre-Trident/v2 constants remain unchanged.

Authenticated commitments now travel on the transaction gossip topic as
`NetworkMessage::DataCommitment` (Borsh discriminant 33, appended after
`TltCovenant`). Full blocks still carry `Block.data_commitments`. Named
typed compact (v27) carries DA short ids under their own lane kind; UTXO
compact is not used. Detached multisign attachments still force a full body.

## TLT inclusion fee

Architecture assigns DA bytes/state growth to TLT base-network fees.
`DA_INCLUSION_FEE_TLT` is 1_000_000 base units (0.01 TLT at 8 decimals) per
**Accepted** authorization. Apply:

1. Selects the operator's P2PKH TLT UTXOs in outpoint-key order (plus any
   same-block outputs to that address).
2. Spends enough to cover the fee. Change returns to the operator at
   `da_fee_change_outpoint(authorization_id)`.
3. Increments the TLT burned-supply counter by the fee. The fee is not paid to
   miners and does not change the coinbase budget.
4. Journals spent/created UTXOs and the prior burned counter so reorg reverts.

Insufficient TLT fails the block (`DA inclusion fee: insufficient TLT`). Exact
duplicates and conflict-lost outcomes do not debit. Mempool/RPC admission
rejects when `balance_of(operator) < DA_INCLUSION_FEE_TLT`.

`TxAuthContext` still requires an explicit DA network fingerprint because the
authorization preimage binds the mesh. Default node boot sets that fingerprint
from the live network fingerprint. A missing fingerprint is a signing-context
error (`data commitment requires a DA network fingerprint`), not a fee-policy
placeholder. `AGORA_ENABLE_DA_LANE` is not used.

`agora_submitDataCommitment` / gossip admission use the same fingerprint and
fee checks. `agora_getDataCommitment` is a public read and reports `pending`,
`accepted`, `confirmed` (work depth), `finalized` (TLT PoW ∧ ≥⅔ OVL ∧ ≥⅔ DRC),
`conflict_lost`, `reverted`, or `unknown`. It never treats lab `recordDa` as
L1 finality (`lab_record_da: false`).

The 64-commitment and 1 MB block caps are consensus resource limits in addition
to the TLT fee. Test-only/custom Trident contexts can omit the fingerprint to
prove the signing-context reject.

## Remaining activation work

The TLT inclusion fee is live on default Experimental boot. Remaining work:

1. Add a durable `agora-layers` outbox that writes intent before submission,
   retries exact payloads, records returned authorization ids, resumes after
   restart, and never converts timeout/confirmations into a finality claim.
2. Only after accepted L1 provenance exists: add a separate bounded read-only
   district service with explicit `canonical_l1: false` and `maturity:
   "Experimental"` fields, cursor pagination, response-size caps, rate limits,
   an explicit public-bind gate, and authentication policy. It must not route
   mint, credit, claim, payment, or other lab mutations.

Until then, keep `AGORA_LAYERS_BIND` on loopback. The binary rejects
non-loopback binds because its mixed lab RPC cannot meet the public endpoint
policy.

## Verification scope

Tests cover the PR #120 payload guarantees plus body-root sensitivity, legacy
block decoding, stable enum discriminants, network/signature hard failures,
state-enforced replay/idempotency, deterministic duplicate/conflict ordering,
atomic staging/revert, TLT fee debit, fail-without-funds, reorg restore of
UTXOs and burned supply, Virtual blue-order winner changes, and restart recovery.
