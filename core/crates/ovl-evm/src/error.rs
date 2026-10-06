//! Errors from the pinned OVL execution lane.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EvmError {
    #[error("{0}")]
    Rejected(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
}

impl EvmError {
    pub fn rejected(message: impl Into<String>) -> Self {
        Self::Rejected(message.into())
    }
}
