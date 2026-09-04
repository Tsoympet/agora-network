# Agora Network — Project Structure

Workspace boundaries for the Agora Network monorepo. Do not place code outside its owning layer without updating this file.

**Architecture target:** [Agora Trident L1](docs/architecture/TRIDENT_L1.md) — one hybrid L1 with TLT/OVL/DRC native. Layer crates below are historical lab/reuse sources until retirement.

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
│   │   ├── kheavyhash/     # Audited Kaspa kHeavyHash digest (vendored)
│   │   ├── state-machine/  # RocksDB, Triple-Zone Logic
│   │   ├── p2p/            # Gossipsub, Mempool, Compact Blocks
│   │   ├── rpc/            # HTTP JSON-RPC methods + dispatcher
│   │   ├── crypto/         # BIP-39/44 Wallet, secp256k1 Signatures
│   │   ├── governance/     # Constitution, ranks, chambers, proposals, √ voting
│   │   ├── ovolos-rollup/  # Lab: former L2 OVL/revm (reuse → L1 execution)
│   │   ├── bridge-sdk/     # Lab: former L3 DRC payments (reuse → L1 payments)
│   │   ├── intent-engine/  # Lab: L4 intent orchestration
│   │   ├── layers-runtime/ # Lab: composes layer crates for operators
│   │   └── miner-sidecar/  # Standalone Binary for CPU Mining (TLT)
│   ├── layers-bin/         # agora-layers lab JSON-RPC (not canonical money)
│   └── node-bin/           # Main entry point for the L1 node
├── docs/                   # Architectural & Scaling Blueprints
│   ├── architecture/       # Trident L1 design freeze
│   ├── core/               # One markdown file per major core module
│   ├── ops/                # Network deployment and Trident readiness gates
│   ├── security/           # Threat model and security requirements
│   ├── testing/            # Trident validation plans and CI coverage
│   └── scaling/            # Historical layer overview (deprecated locus)
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
| `core/crates/kheavyhash` | `keccak` (vendored Kaspa algo) | Networking, consensus policy |
| `core/crates/governance` | `types` | Networking, UI, storage |
| `core/crates/ovolos-rollup` | `types` | P2P, UI |
| `core/crates/bridge-sdk` | `types` | Consensus internals, UI |
| `core/crates/intent-engine` | `types`, `bridge-sdk` | Consensus internals, UI |
| `core/crates/layers-runtime` | `types`, L2/L3/L4 crates | Consensus internals, RocksDB, UI |
| `core/layers-bin` | `layers-runtime` | L1 consensus / RocksDB |
| `core/crates/consensus` | `types`, `crypto`, `kheavyhash` | Disk I/O, UI, RPC transport |
| `core/crates/state-machine` | `types`, `crypto`, `consensus` | libp2p, UI |
| `core/crates/p2p` | `types`, `consensus` | RocksDB, UI |
| `core/crates/rpc` | `types`, node services | Mining loops, UI frameworks |
| `core/crates/miner-sidecar` | `types`, `crypto`, `consensus` (RandomX hasher), `rpc` | Full state machine / RocksDB |
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
