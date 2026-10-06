# TLT performance report

**Status line:** `TALANTON BITCOIN FUNCTIONAL PARITY — INCOMPLETE`

**Maturity:** Experimental library timings. These figures are one local process. They are not a network, mining, or finality benchmark.

Measurements below are copied from the commands that produced them. Nothing in this file is estimated.

## Host

- Date: 2026-10-06
- `rustc 1.99.0` (`x86_64-unknown-linux-gnu`)
- Profile for the Rust rows: `release` (`cargo run --release -p agora-types --example tlt_perf`)
- Node: `v22.14.0` via `node --experimental-strip-types` on the same machine (see the Node section)
- GNU toolchain: `CC=gcc`, `CXX=g++`

## Rust release

Command: `cargo run --release -p agora-types --example tlt_perf`

The script loop uses an equality signature checker (`pubkey == signature`). It does not time secp256k1. Coin selection searches the 12-coin exhaustive window. Merkle rows prove and verify one leaf in a 4,096-leaf pairwise tree.

| Workload | Iterations | Elapsed |
| --- | --- | --- |
| P2PKH-style `eval_tlt_script` | 10,000 | 1,833 µs |
| `select_tlt_coins` on 12 coins | 1,000 | 312,581 µs |
| Merkle prove + verify, 4,096 leaves | 100 | 70,994 µs |

Per call, from those totals: 0.1833 µs per script eval, 312.581 µs per 12-coin selection, 709.94 µs per 4,096-leaf prove and verify.

Log: the `profile=release` lines in `/opt/cursor/artifacts/tlt-bitcoin-clippy-perf.log`.

## Node

Command: `node --experimental-strip-types` importing `apps/shared/light-client/coinselect.ts` and `tltMerkle.ts`, same iteration counts as the Rust coin-selection and Merkle rows. Leaf bytes are SHA-256 digests from `@noble/hashes`.

| Workload | Iterations | Elapsed |
| --- | --- | --- |
| `selectTltCoins` on 12 coins | 1,000 | 348,011 µs |
| Merkle prove + verify, 4,096 leaves | 100 | 1,932,378 µs |

Per call: 348.011 µs per 12-coin selection, 19,323.78 µs per 4,096-leaf prove and verify.

Log: `/opt/cursor/artifacts/tlt-bitcoin-node-perf.log`.

## Not measured

- RandomX hash rate
- Block apply, reorg, or IBD wall time
- secp256k1 covenant verification throughput
- Multi-node finality latency
