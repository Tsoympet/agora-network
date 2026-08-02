# Agora Network — Project Structure

This document defines **workspace boundaries**. Do not place code outside its owning layer without updating this file.

```
agora-network/
├── apps/                         # Client applications (UI only; no consensus logic)
│   ├── desktop/                  # Tauri + RandomX miner sidecar integration
│   ├── mobile/                   # React Native (Expo) light client / wallet
│   └── explorer/                 # Web BlockDAG visualizer (Tailwind + brand system)
├── core/                         # Rust consensus, networking, and state
│   ├── crates/
│   │   ├── types/                # Shared BlockDAG types + ts-rs bindings
│   │   ├── consensus/            # GHOSTDAG, DAA, PoW verification, emission
│   │   ├── state-machine/        # RocksDB storage, triple-zone state logic
│   │   ├── p2p/                  # libp2p gossipsub, mempool, compact blocks
│   │   ├── rpc/                  # JSON-RPC / REST / CEX gateway surface
│   │   ├── crypto/               # BIP-39/44, secp256k1 signatures (no custom crypto)
│   │   └── miner-sidecar/        # Standalone CPU miner binary (RandomX)
│   └── node-bin/                 # Full node process entrypoint
├── docs/                         # Protocol & algorithm documentation
│   └── core/                     # One markdown file per major core module
├── infrastructure/               # External / ops services (not consensus-critical)
│   ├── dns-seeder/               # Peer discovery phonebook
│   ├── stratum-pool/             # ASIC (kHeavyHash) pool aggregation
│   └── testnet-faucet/           # Devnet liquidity
├── scripts/                      # Build, launch, and monorepo utilities
├── AGORA_MASTER_EXECUTION_ROADMAP.md
├── PROJECT_STRUCTURE.md
├── Cargo.toml                    # Rust workspace root
└── README.md
```

## Boundary Rules

| Layer | May depend on | Must not contain |
| --- | --- | --- |
| `core/crates/types` | `borsh`, `serde`, `ts-rs` | Networking, storage, UI |
| `core/crates/crypto` | `types`, audited crypto crates | Consensus policy, P2P |
| `core/crates/consensus` | `types`, `crypto` | Disk I/O, UI, RPC transport |
| `core/crates/state-machine` | `types`, `consensus` | libp2p, UI |
| `core/crates/p2p` | `types`, `consensus` | RocksDB, UI |
| `core/crates/rpc` | `types`, node services | Mining loops, UI frameworks |
| `core/crates/miner-sidecar` | `types`, `crypto` | Full state machine |
| `core/node-bin` | all core crates | Client UI |
| `apps/*` | RPC / generated TS types | Consensus or RocksDB logic |
| `infrastructure/*` | public RPC / P2P APIs | Core crate internals |

## Shared Types

- Canonical definitions live in `core/crates/types`.
- After changing shared types, regenerate and verify `ts-rs` bindings consumed by `apps/`.
- Wire formats use `borsh` for consensus-critical paths.

## Brand Tokens (clients)

| Token | Value | Use |
| --- | --- | --- |
| Agora Obsidian | `#101218` | Primary background / brand base |
| Burnished Gold | `#C59835` | Accent / CTA |
| Aegean Cyan | `#06BBDF` | Secondary accent / links |
| Display font | Cinzel | Headers / brand |
| UI font | Inter | Body / controls |
