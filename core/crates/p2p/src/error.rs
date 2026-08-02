use thiserror::Error;

#[derive(Debug, Error)]
pub enum P2pError {
    #[error("network error: {0}")]
    Network(String),
}
