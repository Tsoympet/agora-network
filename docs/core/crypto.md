# Crypto (`agora-crypto`)

Wallet and signature primitives for Agora Network.

## Rules

- **secp256k1 only** for public-key cryptography.
- Prefer audited crates: `secp256k1`, `bip39`, `sha2`.
- Never invent custom curves, KDFs, or signature schemes in-repo.

## Current surface

- BIP-39 24-word mnemonic generation and seed derivation
- Keypair from seed (interim; full BIP-44 paths in Phase 1 completion)
- ECDSA sign / verify over SHA-256 digests

## Non-goals

- Proof-of-work hashing (lives under consensus / miner)
- Address checksum / bech32 formatting (follows once address HRP is finalized)
