<p align="center">
  <img src="apps/shared/brand/assets/agora-network.png" alt="Agora Network" width="160" />
</p>

<h1 align="center">Agora Network</h1>

<p align="center">
  <strong>Agora Trident L1</strong> — hybrid BlockDAG: TLT RandomX PoW + dual OVL/DRC PoS finality.<br/>
  Three protocol-native assets: <strong>TLT</strong> · <strong>OVL</strong> · <strong>DRC</strong>. Built in Rust.
</p>

<p align="center">
  <a href="https://github.com/Tsoympet/agora-network/actions/workflows/ci.yml"><img src="https://github.com/Tsoympet/agora-network/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="License" /></a>
  <img src="https://img.shields.io/badge/rust-stable-orange.svg" alt="Rust" />
  <img src="https://img.shields.io/badge/pow-RandomX-informational.svg" alt="PoW" />
  <img src="https://img.shields.io/badge/v2%20testnet-frozen%20genesis-success.svg" alt="Frozen v2 TLT testnet genesis" />
  <img src="https://img.shields.io/badge/Trident%20testnet-NO--GO-lightgrey.svg" alt="Trident testnet NO-GO" />
  <img src="https://img.shields.io/badge/mainnet-not%20frozen-lightgrey.svg" alt="Mainnet" />
</p>

---

## About

**Target architecture:** [Agora Trident L1](docs/architecture/TRIDENT_L1.md) — one canonical Layer 1 with three protocol-native assets. TLT RandomX miners propose and order blocks; OVL and DRC validator sets independently attest finality. Peers sync with headers-first IBD over libp2p; wallets talk JSON-RPC.

| Mark | Locus (target) | Issuance | Role |
| --- | --- | --- | --- |
| **TLT** (Talanton) | L1 UTXO | PoW only | Settlement, security, base network fees |
| **OVL** (Ovolos) | L1 accounts | Genesis + staking reserve (never mined) | Execution gas, builders, technical validators |
| **DRC** (Drachma) | L1 accounts | Genesis + staking/community reserve (never mined) | Payments, merchants, community validators |

See [`docs/architecture/TRIDENT_L1.md`](docs/architecture/TRIDENT_L1.md) and [`docs/assets/NATIVE_ASSETS.md`](docs/assets/NATIVE_ASSETS.md).

> **Status / maturity:** Trident remains **Scaffold**, with implementation
> proceeding through stacked feature branches. Current `main` still runs
> **TLT-only L1 UTXO** plus an in-process `agora-layers` lab stack for
> historical OVL/DRC prototypes — **not** a deployed multi-chain network. The
> frozen testnet badge applies only to v2 TLT; public Trident testnet is
> **NO-GO** pending the [readiness gates](docs/ops/TRIDENT_TESTNET_READINESS.md).
> OVL is **not** Ethereum-equivalent; DRC is **not** XRPL-equivalent.
> **Mainnet is not frozen** — `AGORA_NETWORK=mainnet` refuses to boot.

## Features

- **GHOSTDAG + virtual tip** — blue-set ordering, UTXO apply/reorg journals
- **PoW** — RandomX (CPU; testnet/mainnet policy); kHeavyHash available on `dev` / stratum
- **P2P** — libp2p gossipsub, compact blocks, GetBlock / GetHeaders IBD, durable orphans & headers
- **State** — RocksDB column families (`hot` / `warm` / `archival` / `meta` / `utxo`)
- **JSON-RPC** — tips, blocks, txs, mempool, UTXOs, templates, fee estimate, optional Bearer auth
- **Wallets** — BIP-39/44 (`m/44'/8888'/…`), Bech32m addresses, AES-GCM vault (desktop / mobile)
- **Tooling** — miner sidecar, HTTP seeder, faucet, explorer, Docker Compose

## Networks

| | `dev` | `testnet` | `mainnet` |
| --- | --- | --- | --- |
| Genesis | Free-form | **Frozen** | Not frozen |
| Address HRP | `agoradev` | `agoratest` | `agora` |
| PoW | RandomX (env override OK) | RandomX only | RandomX only (planned) |
| Coin type | `8888` (provisional SLIP-0044) | same | same |

**Testnet genesis hash**

```text
afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98
```

Artifact: [`docs/genesis/testnet.genesis.json`](docs/genesis/testnet.genesis.json)

## Quick start

### Prerequisites

- Rust stable (`rust-toolchain.toml`)
- On Linux: `cmake`, `clang`, `g++` (RandomX / RocksDB)
- Optional: Node 20+ for explorer / desktop / mobile

### Single node (testnet)

```bash
git clone https://github.com/Tsoympet/agora-network.git
cd agora-network

# Verify frozen genesis
cargo run -p agora-node -- genesis verify --network testnet

# Run full node (RocksDB under AGORA_DATA)
AGORA_NETWORK=testnet \
AGORA_RPC_BIND=127.0.0.1:8545 \
cargo run -p agora-node
```

```bash
curl -s http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getDagTips","params":[]}'
```

### Two-node local testnet

```bash
cargo build -p agora-dns-seeder -p agora-node -p agora-miner-sidecar

./scripts/local_testnet.sh wipe-two
# terminals:
./scripts/local_testnet.sh seeder
./scripts/local_testnet.sh node-a
./scripts/local_testnet.sh node-b
./scripts/local_testnet.sh smoke-ibd    # mine N blocks, wait for tip sync
./scripts/local_testnet.sh smoke-tx     # signed tx gossip
```

### Docker

```bash
docker compose up --build seeder node-a
docker compose run --rm miner          # RandomX sidecar
docker compose up faucet               # capped mint faucet
```

See [`docs/ops/PUBLIC_TESTNET.md`](docs/ops/PUBLIC_TESTNET.md).

### Mine (RandomX)

```bash
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-miner-sidecar
```

### Install CLI (releases)

After a `v*` tag is published:

```bash
curl -fsSL https://raw.githubusercontent.com/Tsoympet/agora-network/main/scripts/install.sh | bash
# → ~/.local/bin/{agora-node,agora-layers,agora-miner}
```

Or download `agora-cli-<os>-<arch>.tar.gz` / `.zip` from
[GitHub Releases](https://github.com/Tsoympet/agora-network/releases).
Local package: `./scripts/package-cli.sh`.

### Clients

Apps show **Devnet / Testnet / Mainnet** from the connected node and use matching Bech32 HRPs.
Desktop (Win/macOS/Linux) and mobile (iOS/Android) packaging: [`docs/apps/PLATFORMS.md`](docs/apps/PLATFORMS.md).

```bash
cd apps/explorer && npm install && npm run dev        # BlockDAG explorer
cd apps/desktop  && npm install && npm run tauri:dev  # wallet (Tauri) / npm run dev for browser
cd apps/mobile   && npm install && npm start          # Expo light client
```


### Legacy layer lab runtime (deprecated as money source)

```bash
cargo run -p agora-layers   # in-process lab only — see docs/scaling/OVERVIEW.md + Trident migration docs
```

## Architecture

```text
┌─────────────┐   gossip / GetHeaders / GetBlock   ┌─────────────┐
│  agora-node │ ◄────────────────────────────────► │  agora-node │
└──────┬──────┘                                    └─────────────┘
       │ JSON-RPC
       ▼
┌──────────────┐   ┌─────────────┐   ┌──────────┐
│ miner-sidecar│   │  explorer   │   │  wallets │
│ (RandomX)    │   │  faucet     │   │ desktop/ │
└──────────────┘   └─────────────┘   │ mobile   │
                                     └──────────┘
```

| Layer | Path | Responsibility |
| --- | --- | --- |
| Consensus | `core/crates/consensus` | GHOSTDAG, DAA, PoW, emission |
| State | `core/crates/state-machine` | RocksDB, UTXO, genesis, headers |
| P2P | `core/crates/p2p` | libp2p, mempool, IBD |
| RPC | `core/crates/rpc` | JSON-RPC methods + dispatch |
| Node | `core/node-bin` | Process wiring |
| Clients | `apps/*` | Explorer, desktop, mobile |
| Infra | `infrastructure/*` | Seeder, stratum, faucet |

Full workspace map: [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md)

## Documentation

| Document | Description |
| --- | --- |
| [Trident L1 architecture](docs/architecture/TRIDENT_L1.md) | Canonical hybrid L1 design |
| [Phase 0 audit](docs/architecture/TRIDENT_PHASE0_AUDIT.md) | Gap analysis, impact map, PR sequence |
| [Hybrid finality](docs/consensus/HYBRID_POW_DUAL_POS.md) | PoW + dual PoS checkpoints |
| [Native assets / monetary policy](docs/assets/NATIVE_ASSETS.md) | TLT / OVL / DRC rules |
| [Public testnet ops](docs/ops/PUBLIC_TESTNET.md) | Docker, seeds, PoW policy, TLS |
| [Trident testnet readiness](docs/ops/TRIDENT_TESTNET_READINESS.md) | NO-GO gates, secure operations, incident response |
| [Path to complete chain](docs/governance/PATH_TO_COMPLETE_CHAIN.md) | Historical checklist (pre-Trident) |
| [Constitution (Trident)](docs/governance/AGORA_CONSTITUTION.md) | Chambers + approval matrix |
| [Genesis](docs/genesis/README.md) | Frozen testnet v2 / Trident v3 drafts |
| [RPC](docs/core/rpc.md) · [OpenAPI](docs/core/openapi.yaml) | JSON-RPC surface |
| [Migration OVL/DRC → L1](docs/migration/OVL_DRC_TO_L1.md) | Genesis-native preferred |
| [Threat model](docs/security/THREAT_MODEL.md) | Security requirements |
| [Trident test plan](docs/testing/TRIDENT_TEST_PLAN.md) | Unit / consensus / CI gates |
| [Brand system](docs/brand/BRAND_SYSTEM.md) | Obsidian & Gold |
| [Roadmap](AGORA_MASTER_EXECUTION_ROADMAP.md) | Execution phases |

## Develop & test

```bash
cargo fmt --all -- --check
cargo test --workspace   # prefer CI matrix for RandomX-heavy crates

# Portable CI subset (no cmake):
cargo test -p agora-types -p agora-crypto -p agora-rpc -p agora-state-machine

# Node + RandomX (slow):
cargo test -p agora-node --bin agora-node
```

CI: [`.github/workflows/ci.yml`](.github/workflows/ci.yml)

## Contributing

1. Read [`AGENTS.md`](AGENTS.md) and [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md).
2. Keep consensus crypto on audited crates (`secp256k1`, `bip39`) — no custom curves.
3. Prefer small, reviewable PRs against `main`.
4. Regenerate `ts-rs` bindings when changing `agora-types`.

## Security

See [`SECURITY.md`](SECURITY.md). Report vulnerabilities privately; do not put mainnet value on unfrozen networks.

## License

Licensed under either of:

- Apache License, Version 2.0 — [`LICENSE-APACHE`](LICENSE-APACHE)
- MIT license — [`LICENSE-MIT`](LICENSE-MIT)

at your option.

`agora-kheavyhash` includes ISC-licensed Kaspa algorithm code — see [`core/crates/kheavyhash/LICENSE-ISC`](core/crates/kheavyhash/LICENSE-ISC).

---

<p align="center">
  <sub>Obsidian <code>#101218</code> · Burnished Gold <code>#C59835</code> · Aegean Cyan <code>#06BBDF</code></sub>
</p>
