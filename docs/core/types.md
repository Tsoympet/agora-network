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
| `NativeAssetId` | Protocol-native asset (`0x00` TLT, `0x01` OVL, `0x02` DRC) |
| `NativeAmount` | Asset-tagged amount (cross-asset math rejected) |
| `AssetTxOut` | Trident asset-explicit output (v2 `TxOut` remains TLT-implicit) |
| `TransactionAcceptance` | `Accepted` / `ExactDuplicate` / `ConflictLost` / `Invalid` |
| `AcceptanceBitmap` | Per-block packed acceptance bits |
| `Hash` | 32-byte SHA-256 identifier |
| `Address` | 20-byte account payload (secp256k1-derived); display as Bech32m `agora1…` / `agoratest1…` / `agoradev1…` |
| `OutPoint` / `TxIn` / `TxOut` | UTXO references and outputs |
| `Transaction` | Signed transfer (`public_key` + `signature`) |
| `OvlExecutionTx` | Signed, chain-bound OVL value/execution envelope with gas limits |
| `DrcPaymentTx` / `DrcPaymentOutboxEvent` | Signed DRC settlement and deterministic routing event |
| `TransactionBody` | Signable subset (no auth material) |
| `BlockHeader` / `Block` | Multi-parent DAG header + UTXO/account/stake/execution/payment lanes |
| `TridentHeader` | Offline-only, version-gated commitment for a future Trident block path |

See [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md) and [`../assets/NATIVE_ASSETS.md`](../assets/NATIVE_ASSETS.md).

## IDs

- `Transaction::tx_id()` = SHA-256(borsh(tx))
- `Block::id()` = SHA-256(borsh(header))
- `Block::compute_tx_root` = pairwise merkle over tx ids

## Header version boundary

Frozen v2 continues to encode `BlockHeader` directly as Borsh fields in this
order: `version`, `parents`, `timestamp_ms`, `bits`, `nonce`, `tx_root`.
`BlockHeader::hash`, durable header storage, `Block`, PoW, compact blocks, and
P2P messages still consume those exact bytes. A locked test covers both the
58-byte frozen testnet header and the 134-byte frozen Block encoding.

`TridentHeader` is a separate Rust-only type. Its canonical bytes start with the
fixed 32-byte `agora-trident-header-envelope-v1` domain, followed by an explicit
little-endian encoding version and a length-delimited Borsh v1 payload. The
payload commits protocol and state-transition versions, Block 0 commitment,
artifact and consensus-policy identities, parents, timestamp/difficulty/nonce,
body root, and canonical state root. Decoding rejects the wrong domain, unknown
encoding versions, malformed/trailing bytes, zero roots or identities, and
mismatches against caller-recomputed roots and network identity.

The type intentionally has no serde or `ts-rs` export: it is not shared with
clients and is not accepted by mining, consensus, RPC, or P2P. The offline
state-machine planner now derives its exact Block 0 body and recomputable
live-state root. The state-machine consumer can atomically persist the matching
envelope, plan, and datadir identity and returns a sealed storage-readiness
capability only after durable root/identity verification. Runtime activation
remains blocked until a node loader requires that capability and implements an
explicit consensus/PoW/storage/network protocol switch.

## Change process

1. Edit definitions in `core/crates/types`.
2. Run `cargo test -p agora-types` (regenerates bindings).
3. Update any `apps/` imports of generated bindings.
