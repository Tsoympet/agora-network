# Trident data-commitment consensus lane

**Maturity:** Experimental. The consensus block/state lane exists, but live
fee-policy activation, standalone submission, and public district endpoints do
not.

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
| P2P/mempool/template | Full `Block` propagation carries the lane; compact blocks fall back to full bodies | No standalone DA gossip, mempool selection, or operator submission exists |
| RPC | No submit/lookup method for data commitments; transaction confirmation lookup covers the TLT transaction index | No truthful live submission or confirmation tracking |
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
They contribute to `agora-trident-state-root-v5`. Their prior values are stored
in `UtxoJournal` and apply/revert with acceptance in the same `WriteBatch`, so
reorg and crash recovery use the existing `pending_virtual` protocol.

The Trident protocol fingerprint is v5 and the state-transition version is
`agora-trident-state-v6`. Frozen pre-Trident/v2 constants remain unchanged.
No standalone `NetworkMessage` variant was added: authenticated commitments
travel only inside full blocks, preserving every existing wire-enum
discriminant.

## Fail-closed fee and transport policy

Architecture assigns DA bytes/state growth to TLT base-network fees, but no
amount, sponsorship envelope, or debit rule is currently specified. This
change does not invent one. `TxAuthContext` therefore requires an explicit DA
network fingerprint and all current node boot/RPC contexts leave it absent.
Any DA-bearing block on those paths fails with `data commitment lane disabled
pending TLT base-fee policy`.

The 64-commitment and 1 MB block caps are consensus resource limits, not a fee
schedule. Test-only/custom Trident contexts can activate the lane to prove
consensus and reorg behavior.

## Remaining activation work

Before an operator submitter can be enabled:

1. Specify and review a TLT base-fee/sponsorship rule, commit it in Trident
   genesis/consensus policy, and derive the DA activation context only from
   that policy.
2. Add standalone P2P message validation, bounded mempool admission, deterministic template
   ordering, inclusion eviction, and restart behavior.
3. Add an authenticated submit RPC plus a read RPC whose states distinguish pending,
   accepted, confirmed by work depth, finalized by the full PoW + OVL quorum +
   DRC quorum predicate, conflict-lost, and reverted.
4. Add a durable `agora-layers` outbox that writes intent before submission,
   retries exact payloads, records returned transaction ids, resumes after
   restart, and never converts timeout/confirmations into a finality claim.
5. Only after accepted L1 provenance exists: add a separate bounded read-only
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
