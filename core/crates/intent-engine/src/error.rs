use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IntentError {
    #[error("intent expired")]
    Expired,
    #[error("constraint violated: {0}")]
    Constraint(String),
    #[error("no solver route available")]
    Unsolvable,
    #[error("intent already settled")]
    AlreadySettled,
}
