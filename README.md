# Agora Network

A sovereign, multi-layer BlockDAG blockchain built in Rust.

**Branding:** Obsidian & Gold — Agora Obsidian `#101218`, Burnished Gold `#C59835`, Aegean Cyan `#06BBDF`.

## Documentation

| Doc | Purpose |
| --- | --- |
| [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md) | Workspace boundaries |
| [`AGORA_MASTER_EXECUTION_ROADMAP.md`](AGORA_MASTER_EXECUTION_ROADMAP.md) | Phased architecture plan |
| [`AGENTS.md`](AGENTS.md) | AI / contributor system rules |
| [`docs/core/`](docs/core/) | Per-module protocol notes |

## Repository Structure

```
agora-network/
├── apps/                   # Client applications
│   ├── desktop/            # Tauri + RandomX Sidecar
│   ├── mobile/             # React Native (Expo) Light Client
│   └── explorer/           # Web-based BlockDAG Visualizer
├── core/                   # Rust Consensus & State
│   ├── crates/
│   │   ├── types/          # Shared types + ts-rs bindings
│   │   ├── consensus/      # GHOSTDAG, DAA, PoW, Emission
│   │   ├── state-machine/  # RocksDB, Triple-Zone Logic
│   │   ├── p2p/            # Gossipsub, Mempool, Compact Blocks
│   │   ├── rpc/            # CEX Gateway, JSON-RPC, REST API
│   │   ├── crypto/         # BIP-39/44 Wallet, secp256k1
│   │   └── miner-sidecar/  # Standalone Binary for CPU Mining
│   └── node-bin/           # Main entry point for the node
├── docs/                   # Architectural & Scaling Blueprints
├── infrastructure/         # External services
├── scripts/                # Launch & Build utilities
└── Cargo.toml              # Workspace definitions
```

## Getting Started

```bash
# Optional: ensure directory layout
bash scripts/setup_agora.sh

# Build / check the Rust workspace
cargo check --workspace

# Run unit tests
cargo test --workspace

# Optional: check state-machine RocksDB feature in isolation (agora-node enables it by default)
cargo check -p agora-state-machine --features rocksdb

# GHOSTDAG partition fuzzer
cargo run -p agora-consensus --bin ghostdag_fuzzer -- 64

# Scaling layers (L2/L3/L4)
cargo test -p agora-ovolos-rollup -p agora-bridge-sdk -p agora-intent-engine
```

TypeScript bindings for shared types are generated under `core/crates/types/bindings/` when `agora-types` tests run.


### Binaries

```bash
# Full node (libp2p gossip + HTTP JSON-RPC + PoW admission).
# RocksDB under AGORA_DATA (default data/agora-node). Memory-only:
#   cargo run -p agora-node --no-default-features
# Optional: AGORA_LISTEN, AGORA_BOOTSTRAP, AGORA_DATA, AGORA_RPC_BIND,
#           AGORA_RPC_ALLOW_FUND, AGORA_POW_ALGO, AGORA_TEMPLATE_BITS,
#           AGORA_MINER_ADDRESS, AGORA_PREMINE_ADDRESS, AGORA_MIN_RELAY_FEE
# PeerId persists at $AGORA_DATA/p2p/identity.key across restarts.
cargo run -p agora-node
# Local testnet helpers (single-node + two-node IBD smoke):
#   ./scripts/local_testnet.sh              # print runbook
#   ./scripts/local_testnet.sh up           # single node
#   ./scripts/local_testnet.sh seeder / node-a / node-b / smoke-ibd
# Example: curl -s http://127.0.0.1:8545/rpc -H 'content-type: application/json' \
#   -d '{"id":1,"method":"agora_getDagTips","params":[]}'

# CPU RandomX miner sidecar (polls agora_getBlockTemplate / agora_submitBlock)
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-miner-sidecar

# Bootstrap peer phonebook (optional: AGORA_SEEDER_BIND, AGORA_SEEDER_PEERS)
cargo run -p agora-dns-seeder

# kHeavyHash stratum pool (optional: AGORA_STRATUM_BIND)
cargo run -p agora-stratum-pool

# Testnet faucet → live node UTXO mints (node needs AGORA_RPC_ALLOW_FUND=1)
# optional: AGORA_FAUCET_BIND, AGORA_FAUCET_DRIP, AGORA_FAUCET_COOLDOWN_SECS, AGORA_RPC_URL
cargo run -p agora-testnet-faucet
```

### Clients (brand system)

```bash
# Explorer (Vite + Tailwind + Agora_Brand_System.css)
cd apps/explorer && npm install && npm run dev

# Desktop / mobile shells use Nexus icon + shared brand tokens
# See apps/shared/brand and docs/brand/BRAND_SYSTEM.md
```

## Stack Rules (summary)

- Rust + Tokio; `borsh` serialization; `rocksdb` state
- Crypto: secp256k1 / BIP-39 only (no custom crypto)
- P2P: libp2p
- Mining: RandomX (CPU), kHeavyHash (ASIC)
- Clients: Tailwind + Cinzel / Inter brand system
