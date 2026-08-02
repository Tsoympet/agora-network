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
| `Hash` | 32-byte identifier (block / tx / outpoint refs) |
| `Address` | 20-byte account payload (secp256k1-derived) |
| `Transaction` | Versioned transfer with opaque signature bytes |
| `BlockHeader` | Multi-parent DAG header (parents, bits, nonce, tx root) |
| `Block` | Header + transactions |

## Change process

1. Edit definitions in `core/crates/types`.
2. Run type export / tests.
3. Update any `apps/` imports of generated bindings.
