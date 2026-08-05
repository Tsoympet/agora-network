# Agora Network — Agent Instructions

Agora Network is a sovereign, multi-layer BlockDAG blockchain.

## Mission
- **Core:** Rust (Consensus, P2P, State Machine)
- **Clients:** Tauri (Desktop), React Native/Expo (Mobile), Web (Explorer)
- **Branding:** Obsidian & Gold — Agora Obsidian `#101218`, Burnished Gold `#C59835`, Aegean Cyan `#06BBDF`

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

## Cursor Cloud specific instructions

This repo is currently a **Rust-only workspace**. The `apps/*` clients (desktop/mobile/explorer) and `infrastructure/{stratum-pool,testnet-faucet}` dirs are empty scaffolds — there is no JS/TS/Node project to install yet. All buildable/runnable code is the Cargo workspace.

- **Toolchain:** pinned to `stable` via `rust-toolchain.toml` (rustup auto-installs it on first cargo invocation). `rustfmt` + `clippy` components are declared there.
- **Standard commands** live in [`README.md`](README.md) (check/test/run/binaries) — use those; they are accurate. The update script only runs `cargo fetch`; the first `cargo build`/`check`/`test` in a fresh VM still compiles the dependency graph (libp2p, secp256k1, etc.) and takes a few minutes.
- **Runnable binaries:** `agora-node` (full node: RocksDB genesis + libp2p gossip + JSON-RPC line protocol), `agora-miner` (sidecar; Phase 6 stub that only prints a startup line), `agora-dns-seeder` (HTTP peer phonebook on `127.0.0.1:18080`, endpoints `/health`, `GET|POST /peers`).
- **Node env vars:** `AGORA_LISTEN` (default `/ip4/0.0.0.0/tcp/16111`), `AGORA_BOOTSTRAP` (comma-separated multiaddrs), `AGORA_DATA` (default `data/agora-node`), `AGORA_RPC_BIND` (default `127.0.0.1:18545`), `AGORA_PREMINE_ADDRESS` (40 hex chars; default non-zero treasury), `AGORA_STORE=memory` (skip RocksDB). To bootstrap a second node to a first, the `AGORA_BOOTSTRAP` multiaddr **must include the `/p2p/<peer-id>` suffix** printed in the first node's boot logs, not just the ip/tcp part.
- **`rocksdb` / C++ gotcha:** `agora-node` enables the `rocksdb` feature. Build/link with `CXX=g++ CC=gcc` when the default `c++` is clang with mismatched libstdc++ headers; also ensure `libstdc++` is available to the linker (`libstdc++-*-dev`). State-machine unit tests can still run without rocksdb; the node binary cannot.
- **P2P identity:** peer id is persisted at `$AGORA_DATA/libp2p.key`. Channels are bounded; `max_peers` is enforced via `libp2p::connection_limits`.
- **Transaction acceptance:** finality/fees/mempool/RPC confirmations go through `agora-consensus::acceptance` + `agora-state-machine::commit_acceptance`. Coinbase maturity (`COINBASE_MATURITY`), `MIN_RELAY_FEE`, and size caps apply to mempool + acceptance. Do not treat GHOSTDAG blue color as tx acceptance. See [`docs/core/acceptance.md`](docs/core/acceptance.md).
- **RPC:** JSON line protocol — `echo '{"method":"agora_getDagTips","params":null}' | nc 127.0.0.1 18545`. Dispatch lives in `agora-rpc::dispatch`.
- **Network fingerprint:** signatures, gossip topics, and datadirs are bound to `NetworkFingerprint`. Reopening a datadir loads `meta/network_fingerprint` instead of re-igniting genesis. Zero premine address is rejected unless `.allow_zero_premine()`.
- **Critical invariants:** (1) signer must own every spent UTXO; (2) `tx_root` must verify; (3) index-0 is always coinbase; (4) GHOSTDAG merge-set is hash-ordered (not receive-order); (5) `agora-node` persists via RocksDB under `AGORA_DATA` (use `AGORA_STORE=memory` only for ephemeral runs).
