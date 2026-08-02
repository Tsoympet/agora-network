# Agora Network: Master Execution Roadmap

This document is the definitive command center. It bridges architectural theory and physical implementation of the Agora Network.

## Phase 1: Foundation (The Ignition) — in progress

**Goal:** Compile the L1 BlockDAG and achieve P2P connectivity.

### Steps

- [x] **Initialize Monorepo:** Workspace root + Cargo members (`types`, `crypto`, `consensus`, `state-machine`, `p2p`, `rpc`, `miner-sidecar`, `node-bin`)
- [x] **Core crate assembly (scaffold):** Compiling stubs for consensus, state-machine, p2p, rpc
- [x] **Cryptography & types:** Shared `borsh`/`ts-rs` types (`Amount`, `Hash`, `Transaction`, `Block`); BIP-39/44 + secp256k1 tx sign/verify
- [ ] **Storage setup:** Initialize RocksDB with the defined column families (hot / warm / archival + metadata)
- [ ] **Genesis ignition:** `GenesisBuilder` for Block 0 and fixed supply caps
- [x] **Consensus core (initial):** GHOSTDAG blue-set ordering + DAA scaffold + leading-zero PoW verify hooks (RandomX / kHeavyHash FFI later)
- [ ] **P2P handshake:** libp2p gossip + DNS seeder; connect the first two nodes

### Stack locks

- Serialization: `borsh` | State: `rocksdb` | P2P: `libp2p` | Crypto: `secp256k1` / `bip39` / `bip32`
- Mining: RandomX (CPU sidecar), kHeavyHash (stratum pool)
- Clients: Tauri desktop, Expo mobile, web explorer

## Phase 2: Branding & Identity (The Look & Feel)

### Assets

- **TLT:** Talanton (Scales)
- **DRC:** Drachma (Helmet)
- **OBL:** Ovolos (Shield/Spears)

### Implementation

- Replace placeholder icons in the UI with branded assets
- Apply `Agora_Brand_System.css` to all frontends
- Set the Nexus Icon (gold `A`) as the primary app icon in Tauri/Expo
- Brand tokens: Obsidian `#101218`, Burnished Gold `#C59835`, Aegean Cyan `#06BBDF` (Cinzel / Inter)

## Phase 3: Launch Security (Anti-Whale & Governance)

- **Governance:** Quadratic voting — `EffectiveVotes = sqrt(RawBalance)`
- **Whale protection:** Enforce the 5% hard cap on voting power
- **Testing:** `ghostdag_fuzzer` stress-tests against partitioned network simulations

## Phase 4: Scaling (Day 2 Growth)

- **4.1 (L2):** Ovolos Optimistic Rollup for EVM smart-contract scaling
- **4.2 (L3):** Bridge-in-a-Box SDK for custom District Chains (gaming / privacy)
- **4.3 (L4):** Intent-Engine for AI-driven asset orchestration

## Directory Structure Reference

See [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md) for workspace boundaries.

```
agora-network/
├── core/                   # Rust Consensus & State
├── apps/
│   ├── desktop/            # Tauri + RandomX Sidecar
│   ├── mobile/             # Expo light client
│   └── explorer/           # BlockDAG Visualization
└── infrastructure/
    ├── dns-seeder/         # Node Discovery
    ├── stratum-pool/       # ASIC Mining
    └── testnet-faucet/     # Dev Liquidity
```

## Immediate Commands

```bash
# Check / test the workspace
cargo check --workspace
cargo test --workspace

# Optional durable storage backend
cargo check -p agora-state-machine --features rocksdb

# Node / miner binaries
cargo run -p agora-node
cargo run -p agora-miner-sidecar
```

> To build an empire, first stabilize the capital. Then, expand the provinces. Then, empower the citizens.
