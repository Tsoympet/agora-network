# Miner Sidecar (`agora-miner-sidecar`)

Standalone CPU miner binary using **RandomX**.

## Why a sidecar

Mining is CPU-heavy and must not share latency-critical threads with consensus, RocksDB, or libp2p gossip. The sidecar polls the node for work templates over RPC and submits solutions back.

## Loop

1. `agora_getBlockTemplate` → full `Block` (parents = tips, `bits` from live DAA, coinbase payout, `tx_root` committed)
2. Search nonces with `RandomXPowHasher` until `leading_zero_bits(digest) >= bits`
3. `agora_submitBlock` with the template block (coinbase included) and the found nonce

```bash
# Terminal A
AGORA_TEMPLATE_BITS=1 AGORA_MINER_ADDRESS=<40-hex> cargo run -p agora-node

# Terminal B
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-miner-sidecar
```

Env: `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`), `AGORA_MINE_POLL_MS` (default `2000`).
Node payout: `AGORA_MINER_ADDRESS` (20-byte hex; default `00…00`).

## ASIC path

kHeavyHash miners connect through `infrastructure/stratum-pool`, not this binary.
