# Agora Network — Agent Instructions

Agora Network is a sovereign BlockDAG **L1** (GHOSTDAG + UTXO + RandomX). Testnet
genesis is frozen in-repo; mainnet is not bootable until freeze. L2–L4 crates are
a runnable in-process stack (`agora-layers`) — do **not** describe them as a
deployed public multi-chain network until ops publish seeds and DA wiring.

## Mission
- **Core:** Rust (Consensus, P2P, State Machine)
- **Clients:** Tauri (Desktop), React Native/Expo (Mobile), Web (Explorer)
- **Branding:** Obsidian & Gold — Agora Obsidian `#101218`, Burnished Gold `#C59835`, Aegean Cyan `#06BBDF`
- **Three native marks (one per layer):** TLT≈Bitcoin pure PoW L1 UTXO; OVL≈Ethereum hybrid (PoW mint + bonded sequencers); DRC≈XRP hybrid (PoW mint + bonded attestor quorum). OVL/DRC are not L1 UTXO asset ids — see `docs/scaling/TOKEN_ROLES.md`

## Technical Stack Rules
- Rust: idiomatic, thread-safe, async (Tokio). Serialization via `borsh`; state via `rocksdb`.
- React: Tailwind CSS + Agora Brand System (Cinzel headers, Inter UI).
- Crypto: **secp256k1 only**. Never implement custom crypto. Prefer audited crates (`bip39`, `secp256k1`).
- P2P: `libp2p` for all network communications.
- Mining: RandomX (CPU), kHeavyHash (ASIC).

## Monorepo Workflow
- Respect workspace boundaries in [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md).
- Shared types live in `core/crates/types`; keep `ts-rs` bindings in sync.
- Check [`AGORA_MASTER_EXECUTION_ROADMAP.md`](AGORA_MASTER_EXECUTION_ROADMAP.md) before inventing architecture.

## Coding Style
- Comments explain **why**, not what.
- Optimize for a sub-second BlockDAG: prefer references and zero-copy; avoid unnecessary clones.
- Every major `core/` module needs a matching markdown file under `docs/`.
