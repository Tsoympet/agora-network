# Agora Network — Agent Instructions

Agora Network is redesigning into **Agora Trident L1**: one canonical hybrid
BlockDAG Layer 1 with three protocol-native assets (TLT, OVL, DRC).

- **Current `main` code:** TLT L1 (GHOSTDAG + UTXO + RandomX) plus an in-process
  `agora-layers` lab stack for historical OVL/DRC prototypes. Testnet genesis v2
  is frozen in-repo; mainnet is not bootable until freeze.
- **Target:** OVL and DRC balances, supply, staking, and transfer rules live in
  the canonical L1 state machine. Do **not** describe OVL as L2-only or DRC as
  L3-only money. Do **not** describe layers as a deployed public multi-chain
  network.
- Architecture: [`docs/architecture/TRIDENT_L1.md`](docs/architecture/TRIDENT_L1.md)
  and [`docs/architecture/TRIDENT_PHASE0_AUDIT.md`](docs/architecture/TRIDENT_PHASE0_AUDIT.md).

## Mission
- **Core:** Rust (Consensus, P2P, State Machine)
- **Clients:** Tauri (Desktop), React Native/Expo (Mobile), Web (Explorer)
- **Branding:** Obsidian & Gold — Agora Obsidian `#101218`, Burnished Gold `#C59835`, Aegean Cyan `#06BBDF`
- **Three native L1 assets:** TLT (only mineable, RandomX); OVL (PoS validators + execution, never mined); DRC (PoS validators + payments, never mined). See `docs/assets/NATIVE_ASSETS.md`.
- **Finality:** TLT PoW work threshold ∧ ≥⅔ OVL stake ∧ ≥⅔ DRC stake (independent quorums; no price oracle).

## Technical Stack Rules
- Rust: idiomatic, thread-safe, async (Tokio). Serialization via `borsh`; state via `rocksdb`.
- React: Tailwind CSS + Agora Brand System (Cinzel headers, Inter UI).
- Crypto: **secp256k1 only**. Never implement custom crypto. Prefer audited crates (`bip39`, `secp256k1`).
- P2P: `libp2p` for all network communications.
- Mining: RandomX (CPU) for **TLT only**; kHeavyHash (ASIC) remains a dev/stratum option — never a silent public-network fallback.

## Monorepo Workflow
- Respect workspace boundaries in [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md).
- Shared types live in `core/crates/types`; keep `ts-rs` bindings in sync.
- Check Trident architecture docs before inventing architecture; use [`AGORA_MASTER_EXECUTION_ROADMAP.md`](AGORA_MASTER_EXECUTION_ROADMAP.md) for historical phase context.
- Preserve consensus hardening from PRs #76–#81. Port acceptance concepts from the side-branch lineage (#82–#84); do **not** wholesale-merge the divergent foundation scaffold into `main`.

## Coding Style
- Comments explain **why**, not what.
- Optimize for a sub-second BlockDAG: prefer references and zero-copy; avoid unnecessary clones.
- Every major `core/` module needs a matching markdown file under `docs/`.
- Maturity labels only: Scaffold · Experimental · Single-node prototype · Multi-node devnet · Public testnet · Audited production · Mainnet ready. No false readiness claims.
