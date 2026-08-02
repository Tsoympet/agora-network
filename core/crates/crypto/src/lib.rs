//! Audited cryptography for Agora Network.
//!
//! All signing uses secp256k1. Do not add custom elliptic-curve or hash constructions here.

mod error;
mod mnemonic;
mod keys;

pub use error::CryptoError;
pub use keys::{KeyPair, PublicKeyBytes, SignatureBytes};
pub use mnemonic::{generate_mnemonic, seed_fingerprint, seed_from_mnemonic};
