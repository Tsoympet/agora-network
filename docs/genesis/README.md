# Canonical genesis

Agora freezes a genesis document **per layer** so peers share the same economic root:

| Layer | Mark | Artifact (testnet) | Artifact (mainnet) |
| --- | --- | --- | --- |
| **L1** BlockDAG | **TLT** | [`testnet.genesis.json`](testnet.genesis.json) | [`mainnet.genesis.draft.json`](mainnet.genesis.draft.json) |
| **L2** Ovolos rollup | **OVL** | [`ovolos.testnet.genesis.json`](ovolos.testnet.genesis.json) | [`ovolos.mainnet.genesis.draft.json`](ovolos.mainnet.genesis.draft.json) |
| **L3** Drachma bridge | **DRC** | [`drachma.testnet.genesis.json`](drachma.testnet.genesis.json) | [`drachma.mainnet.genesis.draft.json`](drachma.mainnet.genesis.draft.json) |

`dev` has no frozen L1 file (env-driven). Layer runtimes default to embedded testnet OVL/DRC genesis when no file is set.

## Token economy (whole units @ 8 decimals)

| Ticker | Name | Layer | Max supply | Role |
| --- | --- | --- | --- | --- |
| **TLT** | Talanton | L1 | **100,000,000** | Native BlockDAG asset (`SupplyCaps.max_supply`) |
| **DRC** | Drachma | L3 | **6,000,000,000** | Medium of exchange / district & bridge ledger |
| **OVL** | Ovolos | L2 | **21,000,000,000** | Rollup gas / micro-unit ledger |

Only **TLT** is created by L1 consensus (`cf_utxo` / emission). **OVL** and **DRC** each boot from their own layer genesis (caps, premine, parent L1 hash). They are **not** L1 UTXO asset ids.

L1 emission mirrors Bitcoin-shaped policy: 50 TLT initial reward, halvings every
210,000 blue-score, 10% premine.

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

## L2 / L3 constants (frozen in docs + Rust)

| Mark | Testnet `genesis_hash` | Loader |
| --- | --- | --- |
| OVL | `440bb8eb…9e01` | `OvolosGenesis` / `AGORA_OVL_GENESIS_FILE` |
| DRC | `2c4217b5…b314` | `DrachmaGenesis` / `AGORA_DRC_GENESIS_FILE` |

Both documents pin `parent_l1_genesis_hash` to the L1 testnet Block 0 hash.

## CLI

```bash
# L1
cargo run -p agora-node -- genesis dump --network testnet
cargo run -p agora-node -- genesis verify --network testnet --file docs/genesis/testnet.genesis.json

# L2 / L3 runtime (loads layer genesis)
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
