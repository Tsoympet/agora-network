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
| `DataAvailabilityCommitment` | Versioned Borsh integrity/provenance payload for explicitly non-canonical source data |
| `DataCommitmentAuthorization` | secp256k1 operator authorization bound to L1 chain, genesis, fingerprint, and replay nonce |
| `TransactionBody` | Signable subset (no auth material) |
| `BlockHeader` / `Block` | Multi-parent DAG header + UTXO/account/stake/execution/payment/data lanes |

See [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md) and [`../assets/NATIVE_ASSETS.md`](../assets/NATIVE_ASSETS.md).

## IDs

- `Transaction::tx_id()` = SHA-256(borsh(tx))
- `Block::id()` = SHA-256(borsh(header))
- `Block::compute_tx_root` = pairwise merkle over tx ids
- `Block::compute_body_root` = legacy root when appended lanes are empty; DA
  entries activate `agora-block-body-v5` over ordered authorization IDs

`DataCommitmentSource` uses explicit stable Borsh discriminants; future variants
must be appended. `Block` deserialization accepts older bodies that end before
later appended lanes, but partial lengths/elements remain invalid.

## Change process

1. Edit definitions in `core/crates/types`.
2. Run `cargo test -p agora-types` (regenerates bindings).
3. Update any `apps/` imports of generated bindings.
