# Canonical genesis

Agora freezes a genesis document **per layer** so peers share the same economic root.
Each mark is **native PoW money on its own layer**:

| Layer | Mark | PoW | Artifact (testnet) | Artifact (mainnet) |
| --- | --- | --- | --- | --- |
| **L1** BlockDAG | **TLT** | RandomX | [`testnet.genesis.json`](testnet.genesis.json) | [`mainnet.genesis.draft.json`](mainnet.genesis.draft.json) |
| **L2** Ovolos rollup | **OVL** | sha256_leading_zero | [`ovolos.testnet.genesis.json`](ovolos.testnet.genesis.json) | [`ovolos.mainnet.genesis.draft.json`](ovolos.mainnet.genesis.draft.json) |
| **L3** Drachma bridge | **DRC** | sha256_leading_zero | [`drachma.testnet.genesis.json`](drachma.testnet.genesis.json) | [`drachma.mainnet.genesis.draft.json`](drachma.mainnet.genesis.draft.json) |

`dev` has no frozen L1 file (env-driven). Layer runtimes default to embedded testnet OVL/DRC genesis when no file is set.

## Token economy (whole units @ 8 decimals)

| Ticker | Name | Layer | Max supply | Native? | PoW | Role |
| --- | --- | --- | --- | --- | --- | --- |
| **TLT** | Talanton | L1 | **100,000,000** | yes | RandomX | BlockDAG UTXO settlement |
| **OVL** | Ovolos | L2 | **21,000,000,000** | yes | sha256_leading_zero | Rollup gas / L2 coinbase |
| **DRC** | Drachma | L3 | **6,000,000,000** | yes | sha256_leading_zero | District & bridge / L3 coinbase |

Only **TLT** is an L1 UTXO asset id (`cf_utxo` / RandomX emission). **OVL** and **DRC** are native on their layers with their own PoW seals, coinbase emission, caps, premine, and parent L1 hash — they are **not** L1 UTXO asset ids.

L1 / L2 / L3 emission mirrors Bitcoin-shaped policy: 50 units initial reward, halvings every
210,000 score/height, 10% testnet premine.

Testnet layer premines (10% of mark cap) use the same treasury pubkey hash as L1
testnet (`ff9ec96f…`).

## Wallet identity (L1 addresses)

| Network | Bech32m HRP | Example | BIP-44 coin type |
| --- | --- | --- | --- |
| mainnet | `agora` | `agora1…` | `8888` (provisional SLIP-0044) |
| testnet | `agoratest` | `agoratest1…` | `8888` (same until SLIP assign) |
| dev | `agoradev` | `agoradev1…` | `8888` |

## L1 constants

Embedded in `agora-state-machine`:

- `TESTNET_GENESIS_HASH_HEX`
- `TESTNET_PREMINE_ADDRESS_HEX`
- `TESTNET_GENESIS_TIMESTAMP_MS` = `1785715200000` (2026-08-03T00:00:00.000Z)
- `TESTNET_GENESIS_BITS` = `0`

## L2 / L3 constants (frozen in docs + Rust, genesis v2)

| Mark | Testnet `genesis_hash` | Loader |
| --- | --- | --- |
| OVL | `538a5d0f…b86d` | `OvolosGenesis` / `AGORA_OVL_GENESIS_FILE` |
| DRC | `e9b8ce67…d01b` | `DrachmaGenesis` / `AGORA_DRC_GENESIS_FILE` |

Both documents pin `parent_l1_genesis_hash` to the L1 testnet Block 0 hash and set
`native=true`, `pow_algorithm=sha256_leading_zero`, `pow_bits=8`.

## CLI

```bash
# L1
cargo run -p agora-node -- genesis dump --network testnet
cargo run -p agora-node -- genesis verify --network testnet --file docs/genesis/testnet.genesis.json

# L2 / L3 runtime (loads layer genesis; mine via agora_layers_mineOvlBlock / mineDrcBlock)
AGORA_OVL_GENESIS_FILE=docs/genesis/ovolos.testnet.genesis.json \
AGORA_DRC_GENESIS_FILE=docs/genesis/drachma.testnet.genesis.json \
  cargo run -p agora-layers
```

## Node / layers boot

| Env | Default | Meaning |
| --- | --- | --- |
| `AGORA_NETWORK` | `dev` | L1 `dev` / `testnet` / `mainnet` |
| `AGORA_GENESIS_FILE` | unset | Load L1 genesis JSON |
| `AGORA_OVL_GENESIS_FILE` | embedded testnet | Load Ovolos L2 genesis JSON |
| `AGORA_DRC_GENESIS_FILE` | embedded testnet | Load Drachma L3 genesis JSON |

## Mainnet freeze

See [`docs/governance/MAINNET_GENESIS_FREEZE.md`](../governance/MAINNET_GENESIS_FREEZE.md).
Freeze order: L1 Talanton → L2 Ovolos → L3 Drachma (each pins the parent L1 hash).
