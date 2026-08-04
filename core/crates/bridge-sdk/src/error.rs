use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BridgeError {
    #[error("unknown district: {0}")]
    UnknownDistrict(String),
    #[error("duplicate message id")]
    DuplicateMessage,
    #[error("insufficient locked amount")]
    InsufficientLock,
    #[error("insufficient district DRC balance")]
    InsufficientDistrict,
    #[error("message not claimable: {0}")]
    NotClaimable(String),
    #[error("invalid light-client proof: {0}")]
    InvalidProof(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("constraint: {0}")]
    Constraint(String),
    #[error("unauthorized attestor")]
    UnauthorizedAttestor,
    #[error("message awaiting attestor quorum")]
    AwaitingQuorum,
}
