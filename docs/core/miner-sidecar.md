# Miner Sidecar (`agora-miner-sidecar`)

Standalone CPU miner binary using **RandomX**.

## Why a sidecar

Mining is CPU-heavy and must not share latency-critical threads with consensus, RocksDB, or libp2p gossip. The sidecar polls the node for work templates over RPC and submits solutions back.

## Loop

1. `agora_getBlockTemplate` → `BlockHeader` (parents = tips, `bits` from `AGORA_TEMPLATE_BITS`)
2. Search nonces with `RandomXPowHasher` until `leading_zero_bits(digest) >= bits`
3. `agora_submitBlock` with `{ header, transactions: [] }`

```bash
# Terminal A
AGORA_TEMPLATE_BITS=1 cargo run -p agora-node

# Terminal B
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-miner-sidecar
```

Env: `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`), `AGORA_MINE_POLL_MS` (default `2000`).

## ASIC path

kHeavyHash miners connect through `infrastructure/stratum-pool`, not this binary.
