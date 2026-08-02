use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StratumError {
    #[error("unauthorized worker")]
    Unauthorized,
    #[error("unknown job: {0}")]
    UnknownJob(String),
    #[error("share below target")]
    LowDifficulty,
    #[error("duplicate share")]
    DuplicateShare,
    #[error("protocol error: {0}")]
    Protocol(String),
}
