# Path to a complete working Agora blockchain

Status after Phase 37: Agora is a **working local BlockDAG** (mine, transfer, gossip, headers-first IBD) — not yet a public testnet or mainnet.

## What already works (L1)

- GHOSTDAG ordering, virtual tip, UTXO apply/reorg journals
- RandomX + kHeavyHash PoW verify; DAA wired into templates/admission
- RocksDB zones, prune/archival, durable header index
- libp2p gossip, compact blocks, GetBlock, GetHeaders IBD, orphan pool, seeder
- JSON-RPC (tips/block/tx/mempool/UTXO/balance/submit/template) + optional Bearer token
- Frozen **testnet** genesis v2; mainnet boot refused until freeze
- Miner sidecar, stratum scaffold, faucet (dev mint)
- Desktop/mobile/explorer light clients + **password-sealed mnemonic vault**

Local proof: `./scripts/local_testnet.sh` (`smoke-tx`, `smoke-ibd`, `smoke-ibd-catchup`).

## Finish line checklist

### A — Public testnet (external users can join)

| # | Item | Why |
| --- | --- | --- |
| A1 | Non-trivial PoW floor + DAA (`bits` / `daa_min_level` ≫ 0) | Current testnet `bits: 0` is lab-only |
| A2 | Reachable seed nodes + dialable multiaddrs (not only loopback HTTP seeder) | Outsiders cannot discover peers |
| A3 | Persist orphan pool across restarts | IBD survives node bounce |
| A4 | Release artifacts (Docker / binaries) + multi-host runbook | Ops packaging |
| A5 | CI runs `smoke-ibd` / `smoke-tx` (or equivalent) | Catch regressions before publish |
| A6 | Decide RandomX-only vs kHeavyHash for public testnet | ASICs vs CPU miners |
| A7 | Public faucet with treasury/premine policy (not unbounded mint) | Onboarding without `fundAddress` abuse |

### B — Wallet & miner product surface

| # | Item | Why |
| --- | --- | --- |
| B1 | ~~Encrypted mnemonic vault~~ (**done** Phase 37) | Keys must not live only in React state |
| B2 | Fee estimate RPC + clearer send UX | External wallets need guidance |
| B3 | BIP-44 change chain (not always same external index) | Privacy / standard practice |
| B4 | Desktop Tauri mining sidecar wiring (optional) | End-user CPU mine |
| B5 | Stable RPC docs / OpenAPI; TLS termination story | Third-party wallets & explorers |

### C — Mainnet freeze (hard gate)

| # | Item | Why |
| --- | --- | --- |
| C1 | Register SLIP-0044 (or consciously accept provisional `8888`) | Wallet ecosystem identity |
| C2 | Freeze `mainnet.genesis.json` (premine, timestamp, bits, DAA, emission) | Code already refuses unfrozen mainnet |
| C3 | External security review (consensus / UTXO / P2P / RPC) | Real value at risk |
| C4 | Long soak + adversarial reorg/partition tests | Confidence beyond unit smokes |
| C5 | Ops: monitoring, incident runbooks, tagged release | Production operation |

### D — Later (not required for “complete L1”)

- Production Ovolos L2 / bridge / intent settlement
- Hardware wallets, fee market / RBF, Prometheus metrics
- Real DNS seeds, QUIC/hole-punch, explorer polish

## Recommended sequence

1. **A1–A5** — public testnet readiness  
2. **B2–B5** — external wallet/miner polish (vault done)  
3. **C1–C5** — mainnet freeze gate  

Human decisions required before C: premine/treasury addresses, genesis timestamp, target difficulty, SLIP registration, RandomX vs dual-algo policy.
