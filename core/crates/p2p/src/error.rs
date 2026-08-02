use thiserror::Error;

#[derive(Debug, Error)]
pub enum P2pError {
    #[error("network error: {0}")]
    Network(String),
    #[error("invalid multiaddr: {0}")]
    InvalidMultiaddr(String),
    #[error("gossip error: {0}")]
    Gossip(String),
    #[error("mempool rejected transaction: {0}")]
    MempoolRejected(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("dns seeder error: {0}")]
    Seeder(String),
}
