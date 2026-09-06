# Layers runtime (`agora-layers-runtime` / `agora-layers`)

**Maturity:** Experimental, single-process lab. Non-canonical under Trident.

Historical in-process composition retained for tests, migration evidence, and
reuse of execution/payment semantics. OVL and DRC canonical balances are native
Trident L1 account state, never this runtime's layer ledgers.

## Binary

```bash
cargo run -p agora-layers
# AGORA_LAYERS_BIND=127.0.0.1:8555       # loopback is enforced
# AGORA_LAYERS_CHALLENGE_MS=60000          # optional local override
# AGORA_OVL_GENESIS_FILE=docs/genesis/ovolos.testnet.genesis.json
# AGORA_DRC_GENESIS_FILE=docs/genesis/drachma.testnet.genesis.json
# AGORA_LAYERS_DATA=./data/layers         # durable L2/L3 checkpoint directory
```

Boots historical Ovolos + Drachma lab genesis documents — see
[`docs/genesis/README.md`](../genesis/README.md). These artifacts are not
Trident monetary genesis. When `AGORA_LAYERS_DATA` is set, mutating RPCs persist
`layers-checkpoint.json` (lab OVL tip/ledger/revm snapshots + DRC bridge state).
Tracked Ovolos batches and local `recordDa` flags are not checkpointed, so this
file is not a durable L1 submission outbox.

- `GET /health` — includes `canonical_l1: false` and `maturity: "Experimental"`
- `POST /rpc` — JSON-RPC body

The binary rejects non-loopback binds. Its mixed RPC contains lab mint/credit
mutations and has no public bearer-auth or rate-limit policy; it is not a
public district service.

## RPC methods

| Method | Historical lab domain |
| --- | --- |
| `agora_layers_getInfo` | all |
| `agora_layers_mintOvl` / `agora_layers_getOvlBalance` | L2 |
| `agora_layers_submitBatch` / `agora_layers_recordDa` / `agora_layers_challenge` / `agora_layers_finalizeDue` | L2 |
| `eth_*` (`chainId`, `blockNumber`, `getBalance`, `getTransactionCount`, `getCode`, `getStorageAt`, `call`, `sendRawTransaction`) | L2 |
| `agora_layers_creditDrc` / `agora_layers_lockAndMint` / `agora_layers_claimMint` / `agora_layers_getDrcBalance` | L3 |
| `agora_layers_payDrc` / `agora_layers_pathPayDrc` / tag registry helpers | L3 |
| `agora_layers_submitIntent` / `agora_layers_settleIntent` / `agora_layers_finalizeIntent` | L4 |

`agora_layers_recordDa` records only an unverified in-process operator
assertion. It does not contact `agora-node`, and `agora_layers_finalizeDue`
advances a lab timer status rather than Trident finality.

`eth_sendRawTransaction` accepts **legacy RLP-signed** Ethereum txs (EIP-155)
or the compact `to||value||data` bootstrap encoding for lab use only.

`LayersRuntime::l1_da_commitment_candidate` can deterministically map a known
batch into the source- and genesis-bound prerequisite type. It does not sign or
submit an L1 transaction. The live consensus/RPC blockers are documented in
[`data-availability.md`](data-availability.md).
