//! Peer-to-peer networking for Agora via **libp2p** only.
//!
//! Provides gossipsub topics, mempool admission, compact-block IBD helpers,
//! and a swarm runtime used by `agora-node`.

mod config;
mod error;
mod fingerprint;
mod getblock;
mod getheaders;
mod ibd;
mod identity;
mod limits;
mod mempool;
mod messages;
mod network;
mod scoring;
mod seeder;
mod topics;

pub use config::NetworkConfig;
pub use fingerprint::{
    fingerprint_topic_tag, network_fingerprint, PROTOCOL_VERSION, STATE_TRANSITION_VERSION,
    TX_SIGNING_VERSION,
};
pub use error::P2pError;
pub use getblock::{getblock_protocol, GetBlockRequest, GetBlockResponse, GETBLOCK_PROTOCOL};
pub use getheaders::{
    getheaders_protocol, validate_header_chain, GetHeadersRequest, GetHeadersResponse,
    GETHEADERS_PROTOCOL, MAX_HEADERS_PER_RESPONSE, MAX_LOCATOR_HASHES,
};
pub use ibd::{
    drain_orphans_after, reconstruct_compact_block, short_ids_for_block, tx_short_id, FetchReason,
    OrphanEntry, OrphanPool, PendingFetches, ReconstructError,
};
pub use identity::{load_or_generate_identity, save_identity};
pub use libp2p::PeerId;
pub use limits::{connection_limits_behaviour, connection_limits_for_max_peers};
pub use mempool::{Mempool, DEFAULT_MIN_RELAY_FEE, DEFAULT_TEMPLATE_TX_LIMIT};
pub use messages::NetworkMessage;
pub use network::{dial_addr, NetworkEvent, NetworkHandle, NetworkNode};
pub use scoring::{
    agora_peer_score_params, agora_topic_score_params, GossipTuning, APP_SCORE_BAD_PEER,
    APP_SCORE_GOOD_PEER,
};
pub use seeder::{
    fetch_seeder_peers, fetch_seeder_peers_best_effort, merge_bootstrap_peers,
    normalize_seeder_url, register_with_seeder, SeederBook,
};
pub use topics::{
    blocks_topic, transactions_topic, NetworkTopics, TOPIC_BLOCKS, TOPIC_TRANSACTIONS,
    TOPIC_VERSION,
};
