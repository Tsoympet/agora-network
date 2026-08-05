# Path to a complete working Agora blockchain

Status: **in-tree engineering for the three-mark stack is finalized** for public-testnet
readiness. What remains for a live public network is ops deployment + human freeze
decisions. This is **role-complete** (TLT≈Bitcoin, OVL≈Ethereum, DRC≈XRP), not
byte-for-byte protocol clones.

## What already works

### L1 — TLT (Bitcoin-class)

- GHOSTDAG, virtual tip, UTXO reorg journals, admission limits
- RandomX (testnet/mainnet policy) + kHeavyHash (dev/stratum)
- Testnet post-genesis PoW floor (`daa_min_level = 8`; genesis hash unchanged)
- Headers-first IBD, durable headers, durable orphans
- Mempool fee ordering + **higher-fee eviction** when full
- `agora_estimateFee` = mempool median + congestion premium
- JSON-RPC Bearer auth + public-bind gate + per-IP rate limit
- Docker / compose + [`docs/ops/PUBLIC_TESTNET.md`](../ops/PUBLIC_TESTNET.md)
- Wallets: vault, BIP-44 change chain, fee estimate helper
- **Non-mint faucet** via treasury signed spends (`AGORA_FAUCET_MODE=treasury`)

### L2 — OVL (Ethereum-class)

- Optimistic rollup + native OVL PoW mint
- Hybrid bonded sequencers for batch submit/finalize
- Persistent `revm` accounts, CREATE, **contract storage in state root**
- `eth_*` subset including `eth_call`, `eth_getCode`, `eth_getStorageAt`,
  `eth_sendRawTransaction` (**legacy RLP + secp256k1 recovery**, compact fallback)
- **Durable L2 checkpoint** via `AGORA_LAYERS_DATA`

### L3 — DRC (XRP-class)

- Native DRC PoW mint + district balances
- Hybrid bonded attestors + quorum finality
- Payment + destination-tag registry/index
- Path payment + `deliverMin`
- Intent settle with `AwaitingFinality` when quorum is required
- **Durable L3 checkpoint** (same `AGORA_LAYERS_DATA` file)

See [`docs/scaling/TOKEN_ROLES.md`](../scaling/TOKEN_ROLES.md).

## Remaining checklist

### A — Public testnet go-live (ops)

| # | Item | Status |
| --- | --- | --- |
| A1 | Non-trivial PoW floor | **done** (DAA min_level 8; genesis bits still 0) |
| A2 | Reachable seeds + dialable multiaddrs | **scaffolded** — deploy seeder on public IP |
| A3 | Persist orphan pool | **done** |
| A4 | Docker / binaries + runbook | **done** |
| A5 | CI live smoke-ibd | partial (unit catch-up + vault); full RandomX compose smoke is operator-run |
| A6 | RandomX-only public testnet | **done** (documented + locked in ChainParams) |
| A7 | Faucet (non-mint treasury spends) | **done** (`AGORA_FAUCET_MODE=treasury`; mint kept as lab opt-in) |

### B — Wallet / miner / layers

| # | Item | Status |
| --- | --- | --- |
| B1 | Encrypted vault | **done** |
| B2 | Fee estimate RPC (congestion-aware) | **done** |
| B3 | BIP-44 change chain | **done** |
| B4 | Desktop Tauri mining sidecar | optional follow-up |
| B5 | OpenAPI + TLS story | **done** (docs) |
| B6 | Devnet / Testnet / Mainnet badge + HRP wiring | **done** (apps) |
| B7 | Multi-OS desktop + iOS/Android packaging docs | **done** — see [`docs/apps/PLATFORMS.md`](../apps/PLATFORMS.md); store signing is ops |
| B8 | OVL eth_* + L2 mempool + storage roots | **done** |
| B9 | DRC tags / path deliverMin / intent finality | **done** |
| B10 | Signed RLP Ethereum txs on OVL | **done** (legacy + EIP-155) |
| B11 | Durable L2/L3 checkpoints | **done** (`AGORA_LAYERS_DATA`) |
| B12 | Civic constitution + ranks + chambers + proposal engine | **done** (crate + docs; node RPC wiring follow-up) |

### C — Mainnet freeze (needs humans)

| # | Item | Status |
| --- | --- | --- |
| C1 | SLIP-0044 registration | **human** — see `SLIP0044.md` |
| C2 | Freeze `mainnet.genesis.json` | **human** — premine, timestamp, bits |
| C3 | External security review | **human** |
| C4 | Long soak / adversarial tests | run before freeze |
| C5 | Ops monitoring / tagged release | after freeze |

Node already refuses `AGORA_NETWORK=mainnet` until genesis is frozen.

## Deferred (explicitly out of scope / later)

- Full Ethereum MPT state roots (SHA-256 account+storage digests today)
- EIP-2718 typed txs beyond legacy (1559 / access-list) on OVL
- XRPL trust lines / issued currencies / DEX order books (native DRC only)
- Governance JSON-RPC + RocksDB persistence + explorer ballot UI (engine + constitution are in-tree)
- Non-mint faucet via separate cold treasury ops runbook polish

## What you must decide / do outside the repo

1. Premine / treasury addresses and amounts for mainnet  
2. Genesis timestamp and initial `bits` / DAA floor for mainnet  
3. File or accept provisional SLIP-0044 coin type `8888`  
4. Deploy public seeder + 2+ nodes; publish seeder URL + genesis hash  
5. Commission a security review before real value
