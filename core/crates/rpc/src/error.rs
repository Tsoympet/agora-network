use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RpcError {
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl RpcError {
    pub fn code(&self) -> i64 {
        match self {
            Self::MethodNotFound(_) => -32601,
            Self::InvalidParams(_) => -32602,
            Self::NotFound(_) => -32004,
            Self::Rejected(_) => -32001,
            Self::Internal(_) => -32603,
        }
    }
}
