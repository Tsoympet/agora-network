//! Peer-to-peer networking for Agora via **libp2p** only.
//!
//! Provides gossipsub topics, mempool admission, compact-block IBD helpers,
//! and a swarm runtime used by `agora-node`.

mod config;
mod error;
mod ibd;
mod mempool;
mod messages;
mod network;
mod topics;

pub use config::NetworkConfig;
pub use error::P2pError;
pub use ibd::{
    reconstruct_compact_block, short_ids_for_block, tx_short_id, PendingFetches, ReconstructError,
};
pub use mempool::Mempool;
pub use messages::NetworkMessage;
pub use network::{dial_addr, NetworkEvent, NetworkHandle, NetworkNode};
pub use topics::{blocks_topic, transactions_topic, TOPIC_BLOCKS, TOPIC_TRANSACTIONS};
