use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RollupError {
    #[error("batch sequence gap: expected {expected}, got {got}")]
    SequenceGap { expected: u64, got: u64 },
    #[error("batch already finalized")]
    AlreadyFinalized,
    #[error("challenge window still open")]
    ChallengeWindowOpen,
    #[error("invalid fraud proof: {0}")]
    InvalidFraudProof(String),
    #[error("execution error: {0}")]
    Execution(String),
}
