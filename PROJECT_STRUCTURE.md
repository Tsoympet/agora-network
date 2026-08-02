# Agora Network — Project Structure

Workspace boundaries for the Agora Network monorepo. Do not place code outside its owning layer without updating this file.

```
agora-network/
├── apps/                   # Client applications
│   ├── shared/brand/       # Agora_Brand_System.css, tokens, Nexus + marks
│   ├── desktop/            # Tauri + RandomX Sidecar
│   ├── mobile/             # React Native (Expo) Light Client
│   └── explorer/           # Web-based BlockDAG Visualizer
├── core/                   # Rust Consensus & State
│   ├── crates/
│   │   ├── types/          # Shared BlockDAG types + ts-rs bindings
│   │   ├── consensus/      # GHOSTDAG, DAA, PoW, Emission
│   │   ├── state-machine/  # RocksDB, Triple-Zone Logic
│   │   ├── p2p/            # Gossipsub, Mempool, Compact Blocks
│   │   ├── rpc/            # CEX Gateway, JSON-RPC, REST API
│   │   ├── crypto/         # BIP-39/44 Wallet, secp256k1 Signatures
│   │   ├── governance/     # Quadratic voting + anti-whale caps
│   │   └── miner-sidecar/  # Standalone Binary for CPU Mining
│   └── node-bin/           # Main entry point for the node
├── docs/                   # Architectural & Scaling Blueprints
│   └── core/               # One markdown file per major core module
├── infrastructure/         # External services
│   ├── dns-seeder/         # Node discovery phonebook
│   ├── stratum-pool/       # ASIC Mining aggregation
│   └── testnet-faucet/     # Dev Liquidity
├── scripts/                # Launch & Build utilities
├── Cargo.toml              # Workspace definitions
└── README.md
```

## Boundary Rules

| Layer | May depend on | Must not contain |
| --- | --- | --- |
| `core/crates/types` | `borsh`, `serde`, `ts-rs` | Networking, storage, UI |
| `core/crates/crypto` | `types`, audited crypto crates | Consensus policy, P2P |
| `core/crates/governance` | `types` | Networking, UI, storage |
| `core/crates/consensus` | `types`, `crypto` | Disk I/O, UI, RPC transport |
| `core/crates/state-machine` | `types`, `consensus` | libp2p, UI |
| `core/crates/p2p` | `types`, `consensus` | RocksDB, UI |
| `core/crates/rpc` | `types`, node services | Mining loops, UI frameworks |
| `core/crates/miner-sidecar` | `types`, `crypto` | Full state machine |
| `core/node-bin` | all core crates | Client UI |
| `apps/*` | RPC / generated TS types | Consensus or RocksDB logic |
| `infrastructure/*` | public RPC / P2P APIs | Core crate internals |

## Shared Types

- Canonical definitions live in `core/crates/types`
- After changing shared types, regenerate and verify `ts-rs` bindings under `core/crates/types/bindings/`
- Wire formats use `borsh` for consensus-critical paths

## Brand Tokens (clients)

| Token | Value | Use |
| --- | --- | --- |
| Agora Obsidian | `#101218` | Primary background / brand base |
| Burnished Gold | `#C59835` | Accent / CTA |
| Aegean Cyan | `#06BBDF` | Secondary accent / links |
| Display font | Cinzel | Headers / brand |
| UI font | Inter | Body / controls |
