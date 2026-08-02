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

# Optional: durable RocksDB backend (needs g++ / libstdc++)
cargo check -p agora-state-machine --features rocksdb

# GHOSTDAG partition fuzzer
cargo run -p agora-consensus --bin ghostdag_fuzzer -- 64

# Scaling layers (L2/L3/L4)
cargo test -p agora-ovolos-rollup -p agora-bridge-sdk -p agora-intent-engine
```

TypeScript bindings for shared types are generated under `core/crates/types/bindings/` when `agora-types` tests run.


### Binaries

```bash
# Full node (libp2p gossip). Optional: AGORA_LISTEN, AGORA_BOOTSTRAP, AGORA_DATA
cargo run -p agora-node

# CPU miner sidecar
cargo run -p agora-miner-sidecar

# Bootstrap peer phonebook (optional: AGORA_SEEDER_BIND, AGORA_SEEDER_PEERS)
cargo run -p agora-dns-seeder

# kHeavyHash stratum pool (optional: AGORA_STRATUM_BIND)
cargo run -p agora-stratum-pool

# Testnet faucet (optional: AGORA_FAUCET_BIND, AGORA_FAUCET_DRIP, AGORA_FAUCET_COOLDOWN_SECS)
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
