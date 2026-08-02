use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("bip32 derivation error: {0}")]
    Bip32(String),
    #[error("secp256k1 error: {0}")]
    Secp256k1(String),
    #[error("invalid signature length")]
    InvalidSignatureLength,
    #[error("invalid public key length")]
    InvalidPublicKeyLength,
    #[error("transaction auth material missing or malformed")]
    InvalidTransactionAuth,
}
