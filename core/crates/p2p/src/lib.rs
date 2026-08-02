//! Peer-to-peer networking for Agora via **libp2p** only.
//!
//! Provides gossipsub topics, mempool admission, compact-block IBD helpers,
//! and a swarm runtime used by `agora-node`.

mod config;
mod error;
mod getblock;
mod ibd;
mod limits;
mod mempool;
mod messages;
mod network;
mod scoring;
mod seeder;
mod topics;

pub use config::NetworkConfig;
pub use error::P2pError;
pub use getblock::{GetBlockRequest, GetBlockResponse, GETBLOCK_PROTOCOL};
pub use ibd::{
    reconstruct_compact_block, short_ids_for_block, tx_short_id, PendingFetches, ReconstructError,
};
pub use limits::{connection_limits_behaviour, connection_limits_for_max_peers};
pub use mempool::{Mempool, DEFAULT_TEMPLATE_TX_LIMIT};
pub use messages::NetworkMessage;
pub use libp2p::PeerId;
pub use network::{dial_addr, NetworkEvent, NetworkHandle, NetworkNode};
pub use scoring::{
    agora_peer_score_params, agora_topic_score_params, GossipTuning, APP_SCORE_BAD_PEER,
    APP_SCORE_GOOD_PEER,
};
pub use seeder::{
    fetch_seeder_peers, fetch_seeder_peers_best_effort, merge_bootstrap_peers, normalize_seeder_url,
    register_with_seeder, SeederBook,
};
pub use topics::{blocks_topic, transactions_topic, TOPIC_BLOCKS, TOPIC_TRANSACTIONS};
