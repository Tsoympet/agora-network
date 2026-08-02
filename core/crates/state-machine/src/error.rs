use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("unknown zone")]
    UnknownZone,
}
