# Types (`agora-types`)

Shared BlockDAG primitives used across consensus, state, P2P, RPC, and clients.

## Why this crate exists

Consensus objects must have a single canonical definition. Clients consume the same shapes via `ts-rs` so wallet/explorer code cannot drift from node encoding.

## Encoding

- **Consensus / storage / P2P:** `borsh` for deterministic, compact binary.
- **RPC / TS bindings:** `serde` + `ts-rs` exports under `bindings/` when export tests run.

## Core objects

| Type | Role |
| --- | --- |
| `Amount` | Base units (8 decimals) |
| `Hash` | 32-byte SHA-256 identifier |
| `Address` | 20-byte account payload (secp256k1-derived) |
| `OutPoint` / `TxIn` / `TxOut` | UTXO references and outputs |
| `Transaction` | Signed transfer (`public_key` + `signature`) |
| `TransactionBody` | Signable subset (no auth material) |
| `BlockHeader` / `Block` | Multi-parent DAG header + txs |

## IDs

- `Transaction::tx_id()` = SHA-256(borsh(tx))
- `Block::id()` = SHA-256(borsh(header))
- `Block::compute_tx_root` = pairwise merkle over tx ids

## Change process

1. Edit definitions in `core/crates/types`.
2. Run `cargo test -p agora-types` (regenerates bindings).
3. Update any `apps/` imports of generated bindings.
