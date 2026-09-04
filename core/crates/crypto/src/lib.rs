//! Audited cryptography for Agora Network.
//!
//! All signing uses secp256k1. Do not add custom elliptic-curve constructions here.
//! BIP-44 paths are realized via the `bip32` crate; secrets are imported into `secp256k1`.

mod address;
mod bip44;
mod error;
mod keys;
mod mnemonic;
mod transaction;

pub use address::address_from_pubkey;
pub use bip44::{
    derive_bip44, Bip44Path, AGORA_COIN_TYPE, AGORA_COIN_TYPE_PROVISIONAL, AGORA_COIN_TYPE_TESTNET,
};
pub use error::CryptoError;
pub use keys::{parse_compressed_public_key, KeyPair, PublicKeyBytes, SignatureBytes};
pub use mnemonic::{generate_mnemonic, seed_fingerprint, seed_from_mnemonic};
pub use transaction::{
    sign_transaction, sign_transaction_bound, signature_from_slice, signer_address,
    verify_transaction, verify_transaction_bound,
};
