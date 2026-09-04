# Crypto (`agora-crypto`)

Wallet and signature primitives for Agora Network.

## Rules

- **secp256k1 only** for public-key cryptography.
- Prefer audited crates: `secp256k1`, `bip39`, `bip32`, `sha2`.
- Never invent custom curves, KDFs, or signature schemes in-repo.

## Current surface

- BIP-39 24-word mnemonic generation and seed derivation
- BIP-44 paths via `bip32` CKD (`m/44'/8888'/account'/change/index`) — provisional SLIP-0044
- Address = first 20 bytes of SHA-256(compressed pubkey)
- String form = **Bech32m** with HRP `agora` (`Address::to_bech32` / light-client `encodeAddress`); hex still accepted at RPC/env boundaries
- ECDSA sign / verify over SHA-256 digests
- Transaction auth: sign `TransactionBody` borsh bytes; attach pubkey + compact signature
- Account-transfer auth (OVL/DRC) and checkpoint attestation sign/verify (`sign_checkpoint_attestation`)

## BIP-44 notes

- `AGORA_COIN_TYPE = 8888` is provisional until a SLIP-0044 code is assigned (recorded in genesis `wallet.coin_type`).
- Bech32m HRPs are network-scoped: mainnet `agora`, testnet `agoratest`, dev `agoradev`.
- Prefer `derive_bip44` over `KeyPair::from_seed` for wallet accounts.

## Non-goals

- Proof-of-work hashing (lives under consensus / miner)
