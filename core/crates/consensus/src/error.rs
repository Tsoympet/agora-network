use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("block not found in DAG: {0}")]
    MissingBlock(String),
    #[error("invalid proof of work")]
    InvalidPow,
    #[error("emission schedule error: {0}")]
    Emission(String),
}
