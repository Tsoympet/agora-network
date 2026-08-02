use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("secp256k1 error: {0}")]
    Secp256k1(String),
    #[error("invalid signature length")]
    InvalidSignatureLength,
}
