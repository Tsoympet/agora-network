# Agora Network: Master Execution Roadmap

This document is the definitive command center. It bridges architectural theory and physical implementation of the Agora Network.

## Phase 1: Foundation (The Ignition) — in progress

**Goal:** Compile the L1 BlockDAG and achieve P2P connectivity.

### Steps

- [x] **Initialize Monorepo:** Workspace root + Cargo members (`types`, `crypto`, `consensus`, `state-machine`, `p2p`, `rpc`, `miner-sidecar`, `node-bin`)
- [x] **Core crate assembly (scaffold):** Compiling stubs for consensus, state-machine, p2p, rpc
- [x] **Cryptography & types:** Shared `borsh`/`ts-rs` types (`Amount`, `Hash`, `Transaction`, `Block`); BIP-39/44 + secp256k1 tx sign/verify
- [x] **Storage setup:** Five column families (`hot` / `warm` / `archival` / `meta` / `utxo`)
- [x] **Genesis ignition:** `GenesisBuilder` for Block 0 and fixed supply caps
- [x] **Consensus core (initial):** GHOSTDAG blue-set ordering + DAA scaffold + leading-zero PoW verify hooks (RandomX / kHeavyHash FFI later)
- [x] **P2P handshake:** libp2p gossipsub + mempool admission + DNS seeder; two-node local gossip test

### Stack locks

- Serialization: `borsh` | State: `rocksdb` | P2P: `libp2p` | Crypto: `secp256k1` / `bip39` / `bip32`
- Mining: RandomX (CPU sidecar), kHeavyHash (stratum pool)
- Clients: Tauri desktop, Expo mobile, web explorer

## Phase 2: Branding & Identity (The Look & Feel) — in progress

### Assets

- [x] **TLT:** Talanton (Scales) — `apps/shared/brand/assets/talanton.svg`
- [x] **DRC:** Drachma (Helmet) — `apps/shared/brand/assets/drachma.svg`
- [x] **OBL:** Ovolos (Shield/Spears) — `apps/shared/brand/assets/ovolos.svg`
- [x] **Nexus Icon:** gold `A` — `apps/shared/brand/assets/nexus-icon.svg` / `.png`

### Implementation

- [x] Apply `Agora_Brand_System.css` across explorer, desktop, and mobile shells
- [x] Set Nexus as primary app icon in Tauri (`src-tauri/icons`) and Expo (`app.json`)
- [x] Explorer branded landing (Cinzel / Inter, Obsidian & Gold)
- Brand tokens: Obsidian `#101218`, Burnished Gold `#C59835`, Aegean Cyan `#06BBDF`

## Phase 3: Launch Security (Anti-Whale & Governance)

- [x] **Governance:** Quadratic voting — `EffectiveVotes = floor(sqrt(RawBalance))` (`agora-governance`)
- [x] **Whale protection:** 5% hard cap on countable balance vs total supply
- [x] **Testing:** `ghostdag_fuzzer` + partition fuzz tests for GHOSTDAG merge scenarios

## Phase 4: Scaling (Day 2 Growth)

- [x] **4.1 (L2):** Ovolos optimistic rollup scaffold (`agora-ovolos-rollup`) — batches, challenge window, fraud-proof hooks, pluggable `EvmExecutor`
- [x] **4.2 (L3):** Bridge-in-a-Box SDK (`agora-bridge-sdk`) — District configs + lock/mint & burn/unlock
- [x] **4.3 (L4):** Intent-Engine scaffold (`agora-intent-engine`) — intents, solver trait, bridge settlement
- [x] Bind production EVM (`revm`) behind `EvmExecutor` (`RevmExecutor`, feature `revm` default-on)
- [x] District light-client merkle proofs + `MessageTransport` / `InMemoryTransport`

## Phase 5: Infrastructure & RPC surface — in progress

- [x] **Stratum pool scaffold:** `infrastructure/stratum-pool` (`agora-stratum-pool`) JSON-lines TCP + share validation (SHA-256 stand-in for kHeavyHash)
- [x] **Testnet faucet scaffold:** `infrastructure/testnet-faucet` rate-limited `/drip` + balance lookup
- [x] **RPC hardening:** `RpcBackend` + `RpcDispatcher` for tips / block / submit / balance / fund
- [x] **Wire HTTP JSON-RPC into `agora-node`:** `NodeBackend` + `AGORA_RPC_BIND` (`POST /rpc`, `/health`)
- [x] **kHeavyHash PoW:** vendored `agora-kheavyhash` (Kaspa ISC) + `KHeavyHashPowHasher` in consensus/stratum
- [x] **RandomX FFI:** `RandomXPowHasher` via `rust-randomx` (feature `randomx`) + miner-sidecar template loop
- [x] **Block admission:** `ChainState::admit_block` runs `PowVerifier`, wired to gossip + `agora_submitBlock`

## Phase 5b: L1 state close-out — done

- [x] **UTXO apply/revert:** `apply_block` / `revert_journal` + `delete_cf`; wired into `admit_block` with coinbase budget
- [x] **DAA wiring:** templates + admission use `Difficulty.level` as `bits`; retarget + `meta/daa_difficulty`
- [x] **Compact blocks / IBD fetch:** `CompactBlock` + `GetBlock` on `BlockAnnounce`; mempool inflate + pending-fetch dedupe

## Phase 5c: P2P hardening — done

- [x] **DNS seeder pull:** `AGORA_DNS_SEEDER` → `fetch_seeder_peers` + dial merge; register dialable addr on listen
- [x] **GetBlock request-response:** `/agora/getblock/1` CBOR RR to announcing peer; gossip fallback on failure
- [x] **Peer scoring & mesh tuning:** 200ms heartbeat, mesh 4–12, flood publish, gossipsub scores + app-score hooks

## Phase 5d: P2P ops polish — done

- [x] **Periodic seeder refresh:** `SeederBook` + `AGORA_SEEDER_REFRESH_SECS` re-fetch/dial/re-register
- [x] **Connection limits:** `libp2p` connection_limits from `max_peers` / `AGORA_MAX_PEERS`

## Phase 6: Clients & visibility — done

- [x] **Explorer live DAG:** poll tips + parent layer from node RPC; SVG live field in `apps/explorer`
- [x] **Desktop / mobile tip sync:** shared `apps/shared/light-client` polls `agora_getDagTips` + `agora_getBlock`

## Phase 7: L1 mempool hardening — in progress

- [x] **Mempool UTXO pre-checks:** `validate_mempool_tx` + reserved outpoints on RPC and gossip admit
- [ ] **Coinbase outputs in mining templates:** include reward payout in `get_block_template` / miner path

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

# Infrastructure
cargo run -p agora-dns-seeder
cargo run -p agora-stratum-pool
cargo run -p agora-testnet-faucet
```

> To build an empire, first stabilize the capital. Then, expand the provinces. Then, empower the citizens.
