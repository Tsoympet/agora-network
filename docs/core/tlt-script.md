# TLT covenant script

**Maturity:** Experimental. The interpreter is a tested library. It is not activated on the live v1 UTXO lane.

## Why this is separate from v1 transfers

Live Talanton transfers are address-locked. `Transaction` version 1 borsh is `version`, outpoints, `value ‖ address` outputs, `nonce`, compressed pubkey, and a 64-byte secp256k1 signature. That encoding is the frozen testnet transaction id preimage. Adding locktime or script bytes to it would change every existing transaction id.

`TltCovenantTx` (version 2, domain `agora-tlt-covenant-v1`) carries per-input `sequence`, `script_sig`, output `script_pubkey`, and `lock_time`. It is not a block-body field. Block admission still applies v1 transfers only.

`nonce` stays on the covenant transaction so Agora replay binding remains available.

## Script

Opcodes are an Agora assignment, not Bitcoin script compatibility. The set is closed: no loops, jumps, or `OP_CAT`. `CHECKMULTISIG` does not consume a dummy stack element. Integers are unsigned little-endian (1..=8 bytes) with no sign bit.

Working limits, not ceremony-frozen: 512-byte scripts, 32 stack items, 64 opcodes, 80-byte pushes, 15 keys.

| Program | Rule |
| --- | --- |
| P2PKH-style | `DUP PUBKEYHASH <20-byte Agora address> EQUALVERIFY CHECKSIG` |
| P2SH | `SHA256 <32-byte redeem hash> EQUAL`, push-only scriptSig, then the redeem script |
| M-of-N | `m <pubkeys> n CHECKMULTISIG` in pubkey order |
| Hashlock / HTLC | `IF` hash path or `ELSE` absolute locktime refund |

`PUBKEYHASH` is the first 20 bytes of SHA-256(compressed pubkey), the same derivation as `Address`. It is not RIPEMD160(SHA256).

CHECKSIG uses `KeyPair::verify` (secp256k1 over SHA-256 of the sighash preimage). The sighash covers version, outpoints, sequences, outputs, locktime, and nonce. It omits `script_sig`. The transaction id hashes the full encoding, including `script_sig`.

## Locks

- Locktimes below `500_000_000` are blue scores. Values at or above that threshold are unix seconds. Header timestamps are milliseconds and must be converted before a time lock is checked.
- `CHECKLOCKTIMEVERIFY` requires the input sequence to be non-final, the same locktime type, `tx.lock_time >= required`, and the spend blue score or median time to have reached `tx.lock_time`.
- `CHECKSEQUENCEVERIFY` uses bit 31 as the disable flag, bit 22 as the time flag, and the low 16 bits as the magnitude. Relative time steps by 512 unix seconds.
- v1 `Transaction::effective_sequence` is `0xFFFFFFFF` and `effective_lock_time` is 0. v1 transactions do not signal replace-by-fee.

## What this module does not do

- It does not replace RandomX, GHOSTDAG, or the address-locked UTXO set.
- It does not move TLT into an account, an ERC-20, or a wrapped asset.
- It does not activate covenant spends in `apply_block`.
