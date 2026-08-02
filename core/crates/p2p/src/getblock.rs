//! Direct peer `GetBlock` request/response protocol (libp2p request-response).
//!
//! Prefer this over gossiping `NetworkMessage::GetBlock` so full block bodies
//! are not flooded across the mesh.

use agora_types::{Block, Hash};
use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};

/// `/agora/getblock/1` — CBOR request/response for IBD body fetch.
pub const GETBLOCK_PROTOCOL: &str = "/agora/getblock/1";

pub fn getblock_protocol() -> StreamProtocol {
    StreamProtocol::new(GETBLOCK_PROTOCOL)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBlockRequest {
    pub hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBlockResponse {
    /// `None` when the peer does not hold the block.
    pub block: Option<Block>,
}

impl GetBlockRequest {
    pub fn new(hash: Hash) -> Self {
        Self { hash }
    }
}

impl GetBlockResponse {
    pub fn found(block: Block) -> Self {
        Self { block: Some(block) }
    }

    pub fn missing() -> Self {
        Self { block: None }
    }
}
