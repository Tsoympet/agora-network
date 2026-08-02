# Agora Master Execution Roadmap

Architecture and delivery sequence for Agora Network. Prefer implementing against this plan over inventing parallel designs.

## Vision

A sovereign, multi-layer BlockDAG with sub-second confirmation targets:
- **Consensus layer:** GHOSTDAG ordering, DAA, dual PoW (RandomX CPU + kHeavyHash ASIC)
- **State layer:** RocksDB-backed triple-zone state machine
- **Network layer:** libp2p gossip, mempool, compact block relay
- **Access layer:** JSON-RPC / REST for wallets, explorer, and CEX gateways
- **Clients:** Tauri desktop, Expo mobile, web explorer (Obsidian & Gold)

## Phase 0 — Foundation (current)

- [x] Monorepo directory layout
- [x] Cargo workspace membership
- [x] Compiling crate stubs for all `core/` members
- [x] `PROJECT_STRUCTURE.md` + this roadmap
- [x] Cursor / agent guidelines committed
- [x] Module docs under `docs/core/`

**Exit criteria:** `cargo check --workspace` succeeds; boundaries and roadmap are documented.

## Phase 1 — Cryptography & Types

- Shared BlockDAG primitives in `core/crates/types` (`Block`, `Transaction`, `Hash`, amounts)
- `borsh` encode/decode for consensus objects
- `ts-rs` bindings for client consumption
- `core/crates/crypto`: BIP-39 mnemonic, BIP-44 paths, secp256k1 sign/verify only

**Exit criteria:** Wallet key derivation + tx signing round-trip tests pass; TS types generate cleanly.

## Phase 2 — Consensus Core

- GHOSTDAG blue-set selection and block ordering (`docs/core/consensus.md`)
- Difficulty Adjustment Algorithm (DAA) for sub-second DAG tips
- PoW verification hooks for RandomX and kHeavyHash
- Emission schedule module (no ad-hoc reward logic elsewhere)

**Exit criteria:** Deterministic GHOSTDAG ordering tests on synthetic DAGs; PoW verify API stable.

## Phase 3 — State Machine

- RocksDB column families for blocks, UTXO/account zones, and metadata
- Triple-zone logic: hot / warm / archival separation
- Atomic apply/revert of consensus-ordered blocks

**Exit criteria:** Restart-safe state apply; property tests for zone transitions.

## Phase 4 — P2P & Mempool

- libp2p identity, gossipsub topics for blocks/txs
- Mempool admission rules aligned with consensus validation
- Compact block / IBD scaffolding

**Exit criteria:** Two local nodes exchange txs and blocks over libp2p.

## Phase 5 — Node Binary & RPC

- `node-bin` wires consensus + state + p2p
- JSON-RPC / REST surface for balances, submit tx, DAG tips
- CEX gateway-friendly endpoints (rate limits, auth hooks)

**Exit criteria:** Single-node smoke test via RPC; faucet can fund a test address.

## Phase 6 — Mining

- `miner-sidecar`: RandomX CPU mining against node template RPC
- `infrastructure/stratum-pool`: kHeavyHash ASIC aggregation
- Template / share validation path documented

**Exit criteria:** CPU sidecar finds valid testnet blocks; stratum accepts mock shares.

## Phase 7 — Clients

- Desktop (Tauri): wallet + optional local miner sidecar
- Mobile (Expo): light client / watch-only + send via RPC
- Explorer: BlockDAG visualizer using brand system (Cinzel / Inter, Obsidian & Gold)

**Exit criteria:** End-to-end send on testnet from desktop or mobile; explorer renders DAG tips.

## Phase 8 — Testnet Hardening

- DNS seeder, faucet, observability
- Sync, reorg, and adversarial peer drills
- Documentation freeze for external integrators

**Exit criteria:** Public testnet runs with external miners and wallet builds.

## Non-Goals (until roadmap says otherwise)

- Custom cryptographic primitives
- Consensus logic inside `apps/`
- Replacing libp2p with an alternate networking stack
- EVM / smart-contract execution (not in scope until a later roadmap revision)

## Dependency Direction

```
types ← crypto ← consensus ← state-machine
                 ↘         ↗
                   p2p → node-bin ← rpc
                              ↑
                        miner-sidecar
```

Apps and infrastructure talk to the node only through RPC/P2P APIs.
