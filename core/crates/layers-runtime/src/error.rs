use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayersError {
    #[error("rollup: {0}")]
    Rollup(String),
    #[error("bridge: {0}")]
    Bridge(String),
    #[error("intent: {0}")]
    Intent(String),
    #[error("bad request: {0}")]
    BadRequest(String),
}
