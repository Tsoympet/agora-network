# Canonical genesis

## Trident L1 (target)

Agora Trident freezes **one** genesis document for the hybrid L1 with three native assets.

The artifact assigns programmable execution only to OVL. DRC allocations seed
contract-free account/payment state and cannot enable a VM, deploy/call,
bytecode, Hook, or contract API.

| Network | Artifact | Status |
| --- | --- | --- |
| Trident testnet | [`trident.testnet.genesis.draft.json`](trident.testnet.genesis.draft.json) | **Draft** (UNFROZEN; Scaffold) |
| Experimental public-testnet | [`trident.experimental.public-testnet.json`](trident.experimental.public-testnet.json) | **Experimental** freeze-ready (generated secp256k1 keys; not ceremony-final; not mainnet) |
| Trident mainnet | TBD | Not bootable until human freeze |

See [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md) and [`../migration/OVL_DRC_TO_L1.md`](../migration/OVL_DRC_TO_L1.md).

Working supply caps (8 decimals): TLT 100M · OVL 21B · DRC 6B whole units. Only **TLT** is mineable.

## Historical artifacts (pre-Trident)

These remain for reproducibility of the layered lab stack. They are **not** the Trident monetary root.
The Ovolos artifact's EVM state is historical OVL-only state. The Drachma
artifacts describe a historical typed payment/PoW prototype and do not grant
canonical DRC a contract or VM capability.

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

The checked-in draft must fail the second command. The Experimental
public-testnet artifact must pass it:

```bash
cargo run -p agora-node -- genesis trident verify \
  --file docs/genesis/trident.experimental.public-testnet.json \
  --mode freeze-ready
```

That file uses a distinct `chain_id`
(`agora-trident-experimental-testnet-1`), generated secp256k1 OVL/DRC
validators, nonzero timestamp, RandomX-only PoW, and dual-PoS genesis sets
so finality can fire after the PoW work threshold. It is **not**
ceremony-final and **not** mainnet. The public draft stays the ceremony
document and remains UNFROZEN.

Freeze-ready validation rejects `UNFROZEN` or malformed hashes, draft/provisional policy labels,
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

A freeze-ready artifact (not the checked-in draft) can now be materialized
into live Block 0 state:

```bash
cargo run -p agora-node -- genesis trident materialize \
  --file PATH --data /tmp/trident-datadir
```

That command fails on the public UNFROZEN draft, as required. On a freeze-ready
file it writes UTXO/account/treasury/validator state, binds a `TridentHeader`
whose state root is `compose_trident_state_root`, and stores datadir identity.
`agora-node` boots that datadir only when `AGORA_TRIDENT_GENESIS_FILE` points
at the same freeze-ready file. `AGORA_GENESIS_FILE` remains v2-only and cannot
be combined with the Trident file.

The Experimental public-testnet file is the checked-in freeze-ready path:

```bash
cargo run -p agora-node -- genesis trident materialize \
  --file docs/genesis/trident.experimental.public-testnet.json \
  --data /tmp/trident-experimental
```

Docker default compose still boots frozen v2. The opt-in profile
`experimental-trident` points `AGORA_TRIDENT_GENESIS_FILE` at this file
and clears `AGORA_GENESIS_FILE`. Do not call that profile Public testnet
or mainnet.

Gossip and mining still use `Block`/`BlockHeader`. Dual-PoS can finalize
on the Experimental artifact because OVL+DRC genesis validators are
populated. A ceremony freeze of `agora-trident-testnet-1` is still
required before declaring Public testnet.

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
`amount`, `start_timestamp_ms`, `cliff_timestamp_ms`, and `end_timestamp_ms`.
The freeze ceremony must also add the selected top-level `bits` value; it is
optional only while the artifact remains a draft.

## CLI (frozen historical L1 v2)

```bash
cargo run -p agora-node -- genesis verify --network testnet
```

The v2 `dump` and `verify` behavior is unchanged. `AGORA_GENESIS_FILE` remains
a v2 loader and does not accept or boot Trident v3.
