//! Peer-to-peer networking for Agora via **libp2p** only.
//!
//! Provides gossipsub topics, mempool admission, compact-block IBD helpers,
//! and a swarm runtime used by `agora-node`.

mod config;
mod error;
mod getblock;
mod ibd;
mod mempool;
mod messages;
mod network;
mod seeder;
mod topics;

pub use config::NetworkConfig;
pub use error::P2pError;
pub use getblock::{GetBlockRequest, GetBlockResponse, GETBLOCK_PROTOCOL};
pub use ibd::{
    reconstruct_compact_block, short_ids_for_block, tx_short_id, PendingFetches, ReconstructError,
};
pub use mempool::Mempool;
pub use messages::NetworkMessage;
pub use libp2p::PeerId;
pub use network::{dial_addr, NetworkEvent, NetworkHandle, NetworkNode};
pub use seeder::{
    fetch_seeder_peers, fetch_seeder_peers_best_effort, merge_bootstrap_peers, normalize_seeder_url,
    register_with_seeder,
};
pub use topics::{blocks_topic, transactions_topic, TOPIC_BLOCKS, TOPIC_TRANSACTIONS};
