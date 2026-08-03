# Layers runtime (`agora-layers-runtime` / `agora-layers`)

In-process composition of L2 + L3 + L4 for operators and integration tests.

## Binary

```bash
cargo run -p agora-layers
# AGORA_LAYERS_BIND=127.0.0.1:8555
# AGORA_LAYERS_CHALLENGE_MS=60000
```

- `GET /health`
- `POST /rpc` — JSON-RPC body

## RPC methods

| Method | Layer |
| --- | --- |
| `agora_layers_getInfo` | all |
| `agora_layers_mintOvl` / `agora_layers_getOvlBalance` | L2 |
| `agora_layers_submitBatch` / `agora_layers_recordDa` / `agora_layers_challenge` / `agora_layers_finalizeDue` | L2 |
| `agora_layers_creditDrc` / `agora_layers_lockAndMint` / `agora_layers_claimMint` / `agora_layers_getDrcBalance` | L3 |
| `agora_layers_submitIntent` / `agora_layers_settleIntent` | L4 |

This runtime does **not** replace `agora-node`. L1 settlement of DA blobs remains an operator step against the BlockDAG node.
