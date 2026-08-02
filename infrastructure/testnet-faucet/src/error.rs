use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FaucetError {
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("rate limited; retry after {0}s")]
    RateLimited(u64),
    #[error("faucet exhausted")]
    Exhausted,
    #[error("backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, FaucetError>;
