# Canonical genesis

Agora freezes Block 0 per public network so every peer shares the same DAG root.
Genesis artifacts are **version 2** documents: Bitcoin/Kaspa-style monetary +
consensus + wallet identity fields, plus the three Agora marks (TLT / DRC / OVL).

| Network | Artifact | Notes |
| --- | --- | --- |
| `testnet` | [`testnet.genesis.json`](testnet.genesis.json) | Frozen Block 0; HRP `agoratest` |
| `mainnet` | [`mainnet.genesis.draft.json`](mainnet.genesis.draft.json) | Draft economics only — not bootable |
| `dev` | *(none)* | Local / CI — premine & timestamp free via env |

## Token economy (whole units @ 8 decimals)

| Ticker | Name | Layer | Max supply | Role |
| --- | --- | --- | --- | --- |
| **TLT** | Talanton | L1 | **100,000,000** | Native BlockDAG asset (`SupplyCaps.max_supply`) |
| **DRC** | Drachma | L2+ | **6,000,000,000** | Medium of exchange / district & bridge (registry) |
| **OVL** | Ovolos | L2 | **21,000,000,000** | Rollup gas / micro-unit brand (registry) |

Only **TLT** is created by L1 consensus today (`cf_utxo` / emission). DRC and OVL
caps are frozen in the genesis document for wallets, explorers, and future L2
issuance — they are not separate L1 asset ids yet.

L1 emission mirrors Bitcoin-shaped policy: 50 TLT initial reward, halvings every
210,000 blue-score, 10% premine.

## Wallet identity

| Network | Bech32m HRP | Example | BIP-44 coin type |
| --- | --- | --- | --- |
| mainnet | `agora` | `agora1…` | `8888` (provisional SLIP-0044) |
| testnet | `agoratest` | `agoratest1…` | `8888` (same until SLIP assign) |
| dev | `agoradev` | `agoradev1…` | `8888` |

Payload bytes are identical across HRPs; only the human-readable prefix changes.
Hex (40-char) remains accepted everywhere.

## Constants

Embedded in `agora-state-machine`:

- `TESTNET_GENESIS_HASH_HEX`
- `TESTNET_PREMINE_ADDRESS_HEX`
- `TESTNET_GENESIS_TIMESTAMP_MS` = `1785715200000` (2026-08-03T00:00:00.000Z)
- `TESTNET_GENESIS_BITS` = `0`

## CLI

```bash
# Write / refresh the committed artifact (run from repo root)
cargo run -p agora-node -- genesis dump --network testnet

# Check embedded constant + optional file
cargo run -p agora-node -- genesis verify --network testnet
cargo run -p agora-node -- genesis verify --network testnet --file docs/genesis/testnet.genesis.json
```

## Node boot

| Env | Default | Meaning |
| --- | --- | --- |
| `AGORA_NETWORK` | `dev` | `dev` / `testnet` / `mainnet` |
| `AGORA_GENESIS_FILE` | unset | Load & verify a genesis JSON artifact |
| `AGORA_EXPECTED_GENESIS` | unset | Extra hex hash check after load/ignite |
| `AGORA_PREMINE_ADDRESS` | `00…00` | **dev only** — ignored on frozen networks |
| `AGORA_GENESIS_TIMESTAMP_MS` | `0` | **dev only** |
| `AGORA_GENESIS_BITS` | `0` | **dev only** |

`scripts/local_testnet.sh` sets `AGORA_NETWORK=testnet`. Wipe `AGORA_DATA*` after changing the frozen genesis hash fields.

## Mainnet freeze

See [`docs/governance/MAINNET_GENESIS_FREEZE.md`](../governance/MAINNET_GENESIS_FREEZE.md) and
[`docs/governance/SLIP0044.md`](../governance/SLIP0044.md). Prep helper:

```bash
./scripts/prepare_mainnet_genesis.sh
```
