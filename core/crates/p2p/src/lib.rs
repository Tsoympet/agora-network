//! Peer-to-peer networking for Agora via **libp2p** only.
//!
//! Provides gossipsub topics, mempool admission, and a swarm runtime used by `agora-node`.

mod config;
mod error;
mod identity;
mod mempool;
mod messages;
mod network;
mod topics;

pub use config::NetworkConfig;
pub use error::P2pError;
pub use identity::load_or_create_identity;
pub use mempool::Mempool;
pub use messages::NetworkMessage;
pub use network::{dial_addr, NetworkEvent, NetworkHandle, NetworkNode, MAX_GOSSIP_MESSAGE_BYTES};
pub use topics::{blocks_topic, transactions_topic, TOPIC_BLOCKS, TOPIC_TRANSACTIONS};

// Re-export for callers wiring fingerprint-bound networks.
pub use agora_types::NetworkFingerprint;
