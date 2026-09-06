# Canonical genesis

## Trident L1 (target)

Agora Trident freezes **one** genesis document for the hybrid L1 with three native assets.

| Network | Artifact | Status |
| --- | --- | --- |
| Trident testnet | [`trident.testnet.genesis.draft.json`](trident.testnet.genesis.draft.json) | **Draft** (UNFROZEN; Scaffold) |
| Trident mainnet | TBD | Not bootable until human freeze |

See [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md) and [`../migration/OVL_DRC_TO_L1.md`](../migration/OVL_DRC_TO_L1.md).

Working supply caps (8 decimals): TLT 100M · OVL 21B · DRC 6B whole units. Only **TLT** is mineable.

## Historical artifacts (pre-Trident)

These remain for reproducibility of the layered lab stack. They are **not** the Trident monetary root.

| Layer (historical) | Mark | Artifact (testnet) | Artifact (mainnet draft) |
| --- | --- | --- | --- |
| L1 BlockDAG | TLT | [`testnet.genesis.json`](testnet.genesis.json) **frozen v2** | [`mainnet.genesis.draft.json`](mainnet.genesis.draft.json) |
| L2 Ovolos lab | OVL | [`ovolos.testnet.genesis.json`](ovolos.testnet.genesis.json) | [`ovolos.mainnet.genesis.draft.json`](ovolos.mainnet.genesis.draft.json) |
| L3 Drachma lab | DRC | [`drachma.testnet.genesis.json`](drachma.testnet.genesis.json) | [`drachma.mainnet.genesis.draft.json`](drachma.mainnet.genesis.draft.json) |

Frozen L1 testnet v2 genesis hash:

```text
afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98
```

Trident requires genesis **v3**, a new `chain_id`, and a new network fingerprint — peers do not silently upgrade from v2.

## Wallet identity (L1 addresses)

| Network | Bech32m HRP | BIP-44 coin type |
| --- | --- | --- |
| mainnet | `agora` | `8888` (provisional SLIP-0044) |
| testnet | `agoratest` | `8888` |
| dev | `agoradev` | `8888` |

Trident wallets use one seed with **separated derivation roles** per asset/validator function (Phase 1+).

## Offline Trident v3 verification

Draft validation strictly parses the v3 schema, validates every populated
field, and prints deterministic Borsh-based identity and fingerprint
candidates:

```bash
cargo run -p agora-node -- genesis trident verify \
  --file docs/genesis/trident.testnet.genesis.draft.json \
  --mode draft
```

This is intentionally distinct from the fail-closed freeze-readiness gate:

```bash
cargo run -p agora-node -- genesis trident verify \
  --file docs/genesis/trident.testnet.genesis.draft.json \
  --mode freeze-ready
```

The checked-in draft must fail the second command. Freeze-ready validation
rejects `UNFROZEN` or malformed hashes, draft/provisional policy labels,
missing timestamp or difficulty selection, empty OVL/DRC validator sets,
invalid compressed secp256k1 validator keys, missing or over-limit
per-validator commissions, missing or zero validator metadata commitments,
zero reserves/treasuries, and allocation-total mismatches. Runtime-policy
preparation additionally requires an explicit blue-score PoW threshold,
validator commission/concentration limits, and complete TLT/staking emission
schedules. It derives DAA, GHOSTDAG, emission, monetary, staking, finality,
consensus-policy-hash, and P2P fingerprint values through one typed conversion.
Any artifact mutation after hashing is rejected.

Neither mode writes the document, freezes it, converts it to v2
`ChainParams`, or starts a node. Ceremony participants must supply allocations,
validator keys, policy values, and final hashes; this tooling does not invent
them. The verifier is **Scaffold** maturity and does not establish Public
testnet readiness.

The integrated runtime is still insufficient for a safe Trident node loader.
Genesis storage has an atomic prepared-batch commit, and a freeze-ready
artifact can now produce a complete typed policy candidate without compiled
policy defaults. It can also prepare a versioned Block 0 commitment (manifest
v4 over the v3 artifact schema, including chain ID, network fingerprint,
artifact timestamp/difficulty, reward-drip policy, and complete validator
registrations) and a lossless Meta envelope that is overlay-verified before a
future loader may append it. Storage envelope v4 includes a separately
versioned Borsh datadir identity, persisted in the
same atomic batch and checked byte-for-byte on reopen. That identity binds the
chain, network fingerprint, artifact, consensus policy, Block 0 commitment,
committed state root, header network identity, and the concrete header hash
when one is available. A separate,
domain- and version-gated `TridentHeader` can now be derived offline from the
verified commitment plus caller-supplied timestamp, difficulty, nonce, and
concrete-body root. It commits the canonical state root and all required
artifact, policy, Block 0, protocol, and state-transition identities. The
legacy `BlockHeader`, `Block`, hashes, P2P bytes, and node boot remain unchanged.

The v2 node rejects any complete or partial Trident identity before loading or
creating its libp2p key, constructing a swarm, or binding RPC. A future Trident
node must derive the expected identity from independently verified inputs and
pass the exact stored-byte comparison at the same startup boundary.

The candidate is still deliberately not accepted by node startup. The offline
`TridentLiveStatePlan` derives a canonical TLT issuance body/outpoint set
and every account, supply, treasury/control, vesting, validator, epoch snapshot,
reward-pool, acceptance, and initial-finality record. Every manifest leaf maps
to one primary versioned key/value; derived runtime indexes cannot substitute
for a missing source mapping. The plan stages only in a COW overlay, proves
per-asset conservation, rereads exact bytes, recomposes its component roots,
and rejects any body/state mismatch against the commitment or header.

The state-machine API can now combine the verified envelope/datadir identity
and exact plan in one durable batch. It performs a full COW preflight, then
independently rederives and rereads every root and identity before returning a
sealed storage-readiness capability. Exact committed reopen is idempotent;
partial or mismatched state is rejected without overwrite.

No current node loader accepts that capability. Consensus, PoW,
vesting-spend, treasury-authorization, P2P, and RPC activation gates remain
mandatory, and the checked-in artifact remains unfrozen. Until one complete
startup path consumes all of them, v3 remains offline-only and
`AGORA_GENESIS_FILE` continues to accept v2 artifacts only.

Populated `genesis_set` entries use:

```json
{
  "consensus_public_key": "<66 lowercase hex characters; compressed secp256k1>",
  "withdrawal_address": "<ceremony-selected network address>",
  "self_bond": 1,
  "commission_bps": null,
  "metadata_hash": "UNFROZEN"
}
```

`null` and `UNFROZEN` are explicit draft placeholders only. Freeze-ready
entries require an explicitly selected integer commission at or below the set
maximum (and the global 10,000 bps bound) plus a nonzero 32-byte metadata hash
as 64 lowercase hexadecimal characters. An explicit zero-percent commission is
valid; an omitted commission is not ceremony-selected. These fields are part
of the artifact identity, policy hash, P2P fingerprint, Block 0 state root, and
Block 0 commitment.

Populated `initial_allocations` entries use `asset`, `address`, and nonzero
`amount`. Populated `vesting_schedules` entries additionally use nonzero
`amount`, `start_timestamp_ms`, `cliff_timestamp_ms`, `end_timestamp_ms`, and
the explicit release policy `linear_from_start_with_cliff_v1`.
The freeze ceremony must also add the selected top-level `bits` value; it is
optional only while the artifact remains a draft.

## CLI (frozen historical L1 v2)

```bash
cargo run -p agora-node -- genesis verify --network testnet
```

The v2 `dump` and `verify` behavior is unchanged. `AGORA_GENESIS_FILE` remains
a v2 loader and does not accept or boot Trident v3.
