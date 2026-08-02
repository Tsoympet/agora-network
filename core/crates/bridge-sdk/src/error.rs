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
}
