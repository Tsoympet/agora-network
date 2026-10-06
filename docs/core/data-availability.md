# Trident data-commitment consensus lane

**Maturity:** Experimental. The consensus block/state lane, standalone gossip,
mempool, template selection, and submit/get RPC exist. Default boot stays
fail-closed until a reviewed TLT inclusion-fee policy (or
`AGORA_ENABLE_DA_LANE=1`) supplies a DA network fingerprint. Live fee-policy
activation and public district endpoints do not.

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
| P2P/mempool/template | `NetworkMessage::DataCommitment` on the tx topic; mempool reserves `(source, sequence)` and operator nonce; templates include the lane only when the DA fingerprint is set; compact still uses full bodies | Default boot still fail-closes inclusion without a fingerprint |
| RPC | `agora_submitDataCommitment` (token-gated) and public `agora_getDataCommitment` with pending/accepted/confirmed/finalized/conflict_lost/reverted | Submit rejects with the fee-policy message unless the fingerprint is set; get never maps lab `recordDa` to finality |
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
aggregate fingerprint is protocol v25 / `agora-trident-state-v22`. Frozen
pre-Trident/v2 constants remain unchanged.

Authenticated commitments now travel on the transaction gossip topic as
`NetworkMessage::DataCommitment` (Borsh discriminant 33, appended after
`TltCovenant`). Full blocks still carry `Block.data_commitments`. Compact
gossip continues to fall back to the full body when the DA lane is nonempty.

## Fail-closed fee and transport policy

Architecture assigns DA bytes/state growth to TLT base-network fees, but no
amount, sponsorship envelope, or debit rule is currently specified. This
change does not invent one. `TxAuthContext` therefore requires an explicit DA
network fingerprint and default node boot/RPC contexts leave it absent.
Any DA-bearing block on those paths fails with `data commitment lane disabled
pending TLT base-fee policy`.

`agora_submitDataCommitment` / gossip admission use the same fail-closed
check. `agora_getDataCommitment` is a public read and reports `pending`,
`accepted`, `confirmed` (work depth), `finalized` (TLT PoW ∧ ≥⅔ OVL ∧ ≥⅔ DRC),
`conflict_lost`, `reverted`, or `unknown`. It never treats lab `recordDa` as
L1 finality (`lab_record_da: false`).

Operators may opt in with `AGORA_ENABLE_DA_LANE=1`, which binds the DA
fingerprint to the node's live mesh fingerprint. That is Experimental: it
does not create a TLT debit schedule.

The 64-commitment and 1 MB block caps are consensus resource limits, not a fee
schedule. Test-only/custom Trident contexts can activate the lane to prove
consensus and reorg behavior.

## Remaining activation work

Transport is wired. Before default public boot can leave the fingerprint on:

1. Specify and review a TLT base-fee/sponsorship rule, commit it in Trident
   genesis/consensus policy, and derive the DA activation context only from
   that policy (instead of the operator opt-in).
2. Add a durable `agora-layers` outbox that writes intent before submission,
   retries exact payloads, records returned authorization ids, resumes after
   restart, and never converts timeout/confirmations into a finality claim.
3. Only after accepted L1 provenance exists: add a separate bounded read-only
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
atomic staging/revert, Virtual blue-order winner changes, and restart recovery.
