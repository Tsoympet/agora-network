# Crypto (`agora-crypto`)

Wallet and signature primitives for Agora Network.

## Rules

- **secp256k1 only** for public-key cryptography.
- Prefer audited crates: `secp256k1`, `bip39`, `bip32`, `sha2`.
- Never invent custom curves, KDFs, or signature schemes in-repo.

## Current surface

- BIP-39 24-word mnemonic generation and seed derivation
- BIP-44 paths via `bip32` CKD (`m/44'/8888'/account'/change/index`)
- Address = first 20 bytes of SHA-256(compressed pubkey)
- ECDSA sign / verify over SHA-256 digests
- Transaction auth: sign `TransactionBody` borsh bytes; attach pubkey + compact signature

## BIP-44 notes

- `AGORA_COIN_TYPE = 8888` is provisional until a SLIP-0044 code is assigned.
- Prefer `derive_bip44` over `KeyPair::from_seed` for wallet accounts.

## Non-goals

- Proof-of-work hashing (lives under consensus / miner)
- Bech32 address string encoding (client / RPC formatting layer)
