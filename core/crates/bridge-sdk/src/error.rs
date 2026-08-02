use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BridgeError {
    #[error("unknown district: {0}")]
    UnknownDistrict(String),
    #[error("duplicate message id")]
    DuplicateMessage,
    #[error("insufficient locked amount")]
    InsufficientLock,
    #[error("message not claimable: {0}")]
    NotClaimable(String),
    #[error("invalid light-client proof: {0}")]
    InvalidProof(String),
    #[error("transport error: {0}")]
    Transport(String),
}
