# TLT covenant script

**Maturity:** Experimental. Covenant programs are admitted on the block covenant lane. The live v1 transfer wire is unchanged.

## Why this is separate from v1 transfers

Live Talanton transfers are address-locked. `Transaction` version 1 borsh is `version`, outpoints, `value ‖ address` outputs, `nonce`, compressed pubkey, and a 64-byte secp256k1 signature. That encoding is the frozen testnet transaction id preimage. Adding locktime or script bytes to it would change every existing transaction id.

`TltCovenantTx` (version 2, domain `agora-tlt-covenant-v1`) carries per-input `sequence`, `script_sig`, output `script_pubkey`, and `lock_time`. A non-empty `tlt_covenants` lane is appended after the post-v4 body lanes and wrapped into the body root as `agora-block-body-v18`. An empty lane is omitted, so frozen v1 block bytes stay the same.

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

## Block admission

Consensus evaluates each covenant input with `sighash_preimage_bound` when a chain id is configured. P2PKH-style outputs are stored as ordinary `TxOut` records so `balance_of` and v1 spends still see them. Other scripts are stored under `tlt/covenant/utxo/` and restored from the UTXO journal on reorg. Covenant fees join the coinbase budget. `agora_submitTltCovenant` and `agora_getTltCovenant` are the relay and query methods. v1 Merkle proofs still use the pairwise transaction-id root, which is not the wrapped header root once a covenant is present.

## What this module does not do

- It does not replace RandomX, GHOSTDAG, or the address-locked v1 wire.
- It does not put locktime or sequence on v1 `Transaction` bytes.
- It does not move TLT into an account, an ERC-20, or a wrapped asset. Wrapped TLT is not implemented.
- It does not gossip loose covenant transactions or put them in compact-block short ids. A full block still carries the lane.
- Script-locked value is not included in `balance_of`. Legacy address outputs have origin blue score 0, so relative locks against them are not meaningful.
