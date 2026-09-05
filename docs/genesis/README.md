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
invalid compressed secp256k1 validator keys, zero reserves/treasuries, and
allocation-total mismatches. It also requires the document's `genesis_hash`
and `network_fingerprint` to equal independently recomputed values.

Neither mode writes the document, freezes it, converts it to v2
`ChainParams`, or starts a node. Ceremony participants must supply allocations,
validator keys, policy values, and final hashes; this tooling does not invent
them. The verifier is **Scaffold** maturity and does not establish Public
testnet readiness.

The integrated runtime is still insufficient for a safe Trident node loader.
Genesis storage now has an atomic prepared-batch commit and rejects malformed,
orphaned, or mismatched datadir identity before fresh writes. However, runtime
staking parameters and the PoW finality threshold still use compiled defaults,
and the artifact consensus identity is not yet the Block 0 hash expected by DAG
bootstrap. Until those policy and identity paths are unified, v3 remains
offline-only and `AGORA_GENESIS_FILE` continues to accept v2 artifacts only.

Populated `genesis_set` entries use:

```json
{
  "consensus_public_key": "<66 lowercase hex characters; compressed secp256k1>",
  "withdrawal_address": "<ceremony-selected network address>",
  "self_bond": 1
}
```

Populated `initial_allocations` entries use `asset`, `address`, and nonzero
`amount`. Populated `vesting_schedules` entries additionally use nonzero
`amount`, `start_timestamp_ms`, `cliff_timestamp_ms`, and `end_timestamp_ms`.
The freeze ceremony must also add the selected top-level `bits` value; it is
optional only while the artifact remains a draft.

## CLI (frozen historical L1 v2)

```bash
cargo run -p agora-node -- genesis verify --network testnet
```

The v2 `dump` and `verify` behavior is unchanged. `AGORA_GENESIS_FILE` remains
a v2 loader and does not accept or boot Trident v3.
