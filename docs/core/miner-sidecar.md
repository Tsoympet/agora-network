# Miner Sidecar (`agora-miner-sidecar`)

Standalone CPU miner binary using **RandomX**.

## Why a sidecar

Mining is CPU-heavy and must not share latency-critical threads with consensus, RocksDB, or libp2p gossip. The sidecar polls the node for work templates over RPC and submits solutions back.

## ASIC path

kHeavyHash miners connect through `infrastructure/stratum-pool`, not this binary.
