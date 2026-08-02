use bip39::{Language, Mnemonic};
use sha2::{Digest, Sha256};

use crate::CryptoError;

/// Generate a 24-word BIP-39 mnemonic using the OS RNG via `bip39`.
pub fn generate_mnemonic() -> Result<String, CryptoError> {
    let mnemonic = Mnemonic::generate_in(Language::English, 24)
        .map_err(|e| CryptoError::InvalidMnemonic(e.to_string()))?;
    Ok(mnemonic.to_string())
}

/// Derive a 64-byte seed from a BIP-39 mnemonic and optional passphrase.
pub fn seed_from_mnemonic(phrase: &str, passphrase: &str) -> Result<[u8; 64], CryptoError> {
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)
        .map_err(|e| CryptoError::InvalidMnemonic(e.to_string()))?;
    let seed = mnemonic.to_seed(passphrase);
    Ok(seed)
}

/// Compact fingerprint of a seed for logging without leaking key material.
pub fn seed_fingerprint(seed: &[u8; 64]) -> String {
    let digest = Sha256::digest(seed);
    hex::encode(&digest[..8])
}
