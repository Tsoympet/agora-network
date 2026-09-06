# Trident data-commitment prerequisite

**Maturity:** Scaffold. No live submission RPC or public district endpoint exists.

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
| L1 body | Explicit TLT, account, stake, OVL-execution, and DRC-payment lanes; no data-commitment lane | A commitment cannot be consensus-recognized |
| L1 state | No DA nonce, idempotency index, commitment root, reorg journal, or acceptance lane | Replay/restart semantics are undefined |
| P2P/mempool/template | No DA message, admission path, selection, or eviction | No live propagation or mining path |
| RPC | No submit/lookup method for data commitments; transaction confirmation lookup covers the TLT transaction index | No truthful live submission or confirmation tracking |
| Layer checkpoint | Persists ledger/runtime snapshots, but not tracked batch commitments or local DA flags | A restarted process cannot reconstruct old submit intent; there is no durable L1 outbox, retry state, tx id, or confirmation state |
| District HTTP surface | Mixed reads with mint/credit/payment mutations, no bearer auth, no rate limiter | It is forced to loopback and is not a public district API |
| Trident boot | Genesis v3 live-state materialization and startup gate remain incomplete | No public Trident network target is available |

The existing OVL execution `data` field is not a substitute: non-empty contract
data is intentionally rejected, and using it as an undocumented memo would
evade a reviewed transaction kind, fee policy, acceptance rules, and state-root
commitment.

## Implemented prerequisite

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

These types are deliberately absent from `Block`, P2P, mempool, state
transition, and RPC. The replay nonce is cryptographically covered but not yet
state-enforced, so the signed authorization is not a transaction and must not
be sent to `agora_submitTransaction`.

## Required activation work

A separate consensus change must define and test all of the following before
an operator submitter can be enabled:

1. A versioned DA lane or reviewed top-level transaction variant, including
   bounded payload/count limits and a TLT base-fee/sponsorship rule.
2. A Trident-only body-root, protocol, transaction-signing, state-transition,
   and network-fingerprint version bump. Frozen genesis-v2 bytes and mesh
   behavior must remain unchanged.
3. Atomic state application for signature/context validation, per-operator
   replay nonce, exact-duplicate idempotency, a commitment index/root,
   acceptance status, and reorg restoration.
4. P2P message validation, bounded mempool admission, deterministic template
   ordering, inclusion eviction, and restart behavior.
5. Authenticated submit RPC plus a read RPC whose states distinguish pending,
   accepted, confirmed by work depth, finalized by the full PoW + OVL quorum +
   DRC quorum predicate, conflict-lost, and reverted.
6. A durable `agora-layers` outbox that writes intent before submission,
   retries exact payloads, records returned transaction ids, resumes after
   restart, and never converts timeout/confirmations into a finality claim.
7. Only after accepted L1 provenance exists: a separate bounded read-only
   district service with explicit `canonical_l1: false` and `maturity:
   "Experimental"` fields, cursor pagination, response-size caps, rate limits,
   an explicit public-bind gate, and authentication policy. It must not route
   mint, credit, claim, payment, or other lab mutations.

Until then, keep `AGORA_LAYERS_BIND` on loopback. The binary rejects
non-loopback binds because its mixed lab RPC cannot meet the public endpoint
policy.

## Verification scope

Tests cover deterministic/domain-separated commitment bytes, provenance-field
sensitivity, validation bounds, secp256k1 signer and L1 identity binding,
cryptographic replay-nonce rejection, deterministic cross-crate conversion,
unknown-batch rejection, explicit non-canonical runtime labels, and fail-closed
layer RPC binding.
