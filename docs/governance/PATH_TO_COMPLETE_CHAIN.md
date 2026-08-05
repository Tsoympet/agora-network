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
| B12 | Civic constitution + ranks + chambers + proposal engine | **done** |
| B13 | Governance Meta CF + JSON-RPC + explorer/desktop ballot | **done** |

### C — Mainnet freeze (needs humans)

| # | Item | Status |
| --- | --- | --- |
| C1 | SLIP-0044 registration | **human** — see `SLIP0044.md` |
| C2 | Freeze `mainnet.genesis.json` | **human** — premine, timestamp, bits |
| C3 | External security review | **human** |
| C4 | Long soak / adversarial tests | run before freeze |
| C5 | Ops monitoring / tagged release | after freeze |

Node already refuses `AGORA_NETWORK=mainnet` until genesis is frozen.

## Consensus hardening (source-review remediation)

Addressing static architecture/security reviews of `main`:

| # | Finding | Status |
| --- | --- | --- |
| 1 | GHOSTDAG merge-set used local **arrival order** | **fixed** — canonical `(blue_score, hash)` merge order + hash-sorted rebuild |
| 2 | Tip / selected-parent used blue **score**, not work | **fixed** — SP + virtual tip rank by cumulative **blue DAG work** (SP + mergeset blues + self), then score, then hash |
| 3 | DAA bit-exponent multiply + node-global bits | **fixed** — parent-contextual `expected_bits(SP)`; hashrate DAA (`work/elapsed`); MTP; `max_level`; `work_from_bits` beyond 63; cache updates only when virtual tip moves |
| 4 | RandomX context thrash / per-header rebuild | **fixed** — epoch cache (cap 4, build outside mutex) + `max_parent_blue_score_lag` |
| 5 | Invalid before full tx/UTXO validation | **fixed** — signatures + UTXO overlay dry-run **before** durable DAG mutation; body/header/tx-index/tips/ghostdag in one WriteBatch; `pending_virtual` for UTXO reorg |
| 6 | Supply vs UTXO journal | **fixed** — same atomic apply batch |
| 7 | `blue_work` = selected-chain only | **fixed** — includes mergeset blue `block_work` |
| 8 | Full blues `HashSet` persisted per block | **mitigated** — durable compact `mergeset_blues` (+ hydrate); in-memory blues still used for anticone (bounded reachability / prune still follow-up) |
| 9 | Tx index last-writer-wins / confirmations ignore virtual | **fixed** — multi-inclusion `txi/` keys; primary pointer refreshed on reorg; confirmations require blue-in-virtual + UTXO journal |
| 10 | Admit ignored configured `PowAlgorithm` | **fixed** — RandomX vs kHeavyHash branch in `verify_pow` |
| 11 | Tx signing lacked network domain | **fixed** — bound verify enforced in admit/mempool when `chain_id` is configured |
| 12 | Governance described as on-chain | **docs** — classified as **admin RPC prototype** (see Deferred) |
| 13 | CI gaps for consensus readiness | **partial** — fmt/portable/selected tests; full node suite, arrival-order, crash-injection, epoch-thrash, Docker smoke still follow-up |
| 14 | Multi-parent UTXO checked only at SP | **fixed** — pre-persist proof of full `blue_order(candidate)` including mergeset blues |
| 15 | Journal fee reconstruction on unapply | **fixed** — persist `fees`/`subsidy`/`coinbase_total` in `UtxoJournal` |
| 16 | Duplicate coinbase outpoints | **fixed** — reject existing outpoints; coinbase nonce commits to parent-set + timestamp + miner entropy |
| 17 | DAA used `f64`/`log2` | **fixed** — integer doubling thresholds only |
| 18 | No-coinbase passed dry-run | **fixed** — require exactly one coinbase |
| 19 | RPC `confirmations.unwrap_or(1)` | **fixed** — return `orphaned` when not virtual-blue |
| 20 | Parent-recency as consensus reject | **fixed** — relay-only soft drop; IBD still admits |
| 21 | Failed reorg cleared pending early | **fixed** — clear marker only after verified restore |
| 22 | Consensus policy not in network identity | **fixed** — `consensus_policy_hash` on genesis artifact + P2P topic fingerprint (genesis ‖ policy ‖ versions) |
| 23 | Conflicting/duplicate sibling txs stalled DAG | **fixed** — virtual apply: first blue-order spend wins; duplicates/conflicts skipped; merge stays live |
| 24 | Bound signing missing in wallet/faucet | **fixed** — TS wallet + treasury faucet use `signing_bytes_bound`; cross-lang vector gated in CI |
| 25 | Overlay issued-supply ignored reverted subsidies | **fixed** — subtract journal.subsidy when unapplying overlay suffix |
| 26 | Full UTXO clone per admit | **mitigated** — copy-on-write overlay (delta map over live store); RocksDB snapshot still follow-up |
| 27 | Template score/work mismatches | **fixed** — simulate candidate GHOSTDAG for subsidy/epoch; tip parents ranked by blue work |
| 28 | Coinbase maturity via primary tx index | **fixed** — resolve creator from journal that created the live outpoint |
| 29 | Legacy journal subsidy=0 after upgrade | **fixed** — bootstrap migrates journals from block bodies |
| 30 | Announce→getblock RandomX thrash | **mitigated** — announce fetches soft age-filter; larger epoch cache + cold-build pacing |

### Remaining hardening follow-ups (tracked)

- Collapse UTXO reorg into the same WriteBatch as body/tips (true single-commit admit).
- Production GHOSTDAG: bounded reachability, pruning points, incremental anticone (drop full in-memory blues).
- Finalized pruning point + state snapshots so pruned nodes cannot accept parents they cannot validate (non-archival nodes are not full consensus validators until then).
- Header-committed UTXO/state root (optional hardening beyond blue_order pre-proof).
- Multi-node arrival-order / partition soak, crash injection, whole-workspace clippy `-D warnings`, required Docker public-testnet smoke.
- Explicit P2P hello exchange of the network fingerprint (topic scoping is live; identify/hello still follow-up).
- True RocksDB snapshot CoW + bounded branch replay for mainnet-scale UTXO admission.
- **Testnet datadir reset recommended** after journal/coinbase-commitment upgrades if migration cannot repair a corrupted issued-supply.

## Deferred (explicitly out of scope / later)

- Full Ethereum MPT state roots (SHA-256 account+storage digests today)
- EIP-2718 typed txs beyond legacy (1559 / access-list) on OVL
- XRPL trust lines / issued currencies / DEX order books (native DRC only)
- **On-chain governance** (signed gov txs, consensus balance snapshots, locked/refundable deposits, height deadlines, deterministic execution, governance state roots, block replication) — current Meta-CF RPC is an administrative prototype
- Non-mint faucet via separate cold treasury ops runbook polish

## What you must decide / do outside the repo

1. Premine / treasury addresses and amounts for mainnet  
2. Genesis timestamp and initial `bits` / DAA floor for mainnet  
3. File or accept provisional SLIP-0044 coin type `8888`  
4. Deploy public seeder + 2+ nodes; publish seeder URL + genesis hash  
5. Commission a security review before real value
