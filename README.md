# Agora Network

A BlockDAG-based decentralized network built with Rust.

## Repository Structure

```
agora-network/
├── apps/                   # Client applications
│   ├── desktop/            # Tauri + RandomX Sidecar
│   ├── mobile/             # React Native (Expo) Light Client
│   └── explorer/           # Web-based BlockDAG Visualizer
├── core/                   # Rust Consensus & State
│   ├── crates/
│   │   ├── consensus/      # GHOSTDAG, DAA, PoW, Emission
│   │   ├── state-machine/  # RocksDB, Triple-Zone Logic
│   │   ├── p2p/            # Gossipsub, Mempool, Compact Blocks
│   │   ├── rpc/            # CEX Gateway, JSON-RPC, REST API
│   │   ├── crypto/         # BIP-39/44 Wallet, Signatures
│   │   └── miner-sidecar/  # Standalone Binary for CPU Mining
│   └── node-bin/           # Main entry point for the node
├── docs/                   # Architectural & Scaling Blueprints
├── infrastructure/         # External services
│   ├── dns-seeder/         # Node discovery phonebook
│   ├── stratum-pool/       # ASIC Mining aggregation
│   └── testnet-faucet/     # Dev Liquidity
├── scripts/                # Launch & Build utilities
├── Cargo.toml              # Workspace definitions
└── README.md
```

## Getting Started

Run the setup script to initialize the monorepo structure:

```bash
bash scripts/setup_agora.sh
```
