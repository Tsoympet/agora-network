# Canonical genesis

Agora freezes Block 0 per public network so every peer shares the same DAG root.

| Network | Artifact | Notes |
| --- | --- | --- |
| `testnet` | [`testnet.genesis.json`](testnet.genesis.json) | Frozen 2026-08-03; premine → BIP-39 `abandon…about` external(0) |
| `dev` | *(none)* | Local / CI — premine & timestamp free via env |
| `mainnet` | *(not frozen)* | `AGORA_NETWORK=mainnet` refuses boot until a genesis is published |

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

`scripts/local_testnet.sh` sets `AGORA_NETWORK=testnet`. Wipe `AGORA_DATA*` after changing the frozen genesis.
