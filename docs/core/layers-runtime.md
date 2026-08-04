# Layers runtime (`agora-layers-runtime` / `agora-layers`)

In-process composition of L2 + L3 + L4 for operators and integration tests.

## Binary

```bash
cargo run -p agora-layers
# AGORA_LAYERS_BIND=127.0.0.1:8555
# AGORA_LAYERS_CHALLENGE_MS=60000          # optional local override
# AGORA_OVL_GENESIS_FILE=docs/genesis/ovolos.testnet.genesis.json
# AGORA_DRC_GENESIS_FILE=docs/genesis/drachma.testnet.genesis.json
# AGORA_LAYERS_DATA=./data/layers         # durable L2/L3 checkpoint directory
```

Boots from Ovolos (L2) + Drachma (L3) genesis documents — see [`docs/genesis/README.md`](../genesis/README.md).
When `AGORA_LAYERS_DATA` is set, mutating RPCs persist `layers-checkpoint.json` (OVL tip/ledger/revm snaps + DRC bridge state).

- `GET /health`
- `POST /rpc` — JSON-RPC body

## RPC methods

| Method | Layer |
| --- | --- |
| `agora_layers_getInfo` | all |
| `agora_layers_mintOvl` / `agora_layers_getOvlBalance` | L2 |
| `agora_layers_submitBatch` / `agora_layers_recordDa` / `agora_layers_challenge` / `agora_layers_finalizeDue` | L2 |
| `eth_*` (`chainId`, `blockNumber`, `getBalance`, `getTransactionCount`, `getCode`, `getStorageAt`, `call`, `sendRawTransaction`) | L2 |
| `agora_layers_creditDrc` / `agora_layers_lockAndMint` / `agora_layers_claimMint` / `agora_layers_getDrcBalance` | L3 |
| `agora_layers_payDrc` / `agora_layers_pathPayDrc` / tag registry helpers | L3 |
| `agora_layers_submitIntent` / `agora_layers_settleIntent` / `agora_layers_finalizeIntent` | L4 |

`eth_sendRawTransaction` accepts **legacy RLP-signed** Ethereum txs (EIP-155) or the compact `to||value||data` bootstrap encoding.

This runtime does **not** replace `agora-node`. L1 settlement of DA blobs remains an operator step against the BlockDAG node.
