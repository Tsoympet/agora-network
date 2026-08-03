//! Headers-first IBD via libp2p request-response (`GetHeaders` / `Headers`).
//!
//! Peers exchange a Bitcoin-style block locator, then receive a batch of
//! [`BlockHeader`]s along the selected-parent spine. Bodies are fetched with
//! the existing GetBlock protocol.

use agora_types::{BlockHeader, Hash};
use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};

/// Soft cap on locator hashes a client may send.
pub const MAX_LOCATOR_HASHES: usize = 32;

/// Soft cap on headers returned in one response.
pub const MAX_HEADERS_PER_RESPONSE: u32 = 2_000;

/// Legacy unscoped protocol name. Prefer [`crate::NetworkTopics::getheaders_protocol`].
pub const GETHEADERS_PROTOCOL: &str = "/agora/dev/getheaders/1";

/// Default (`dev`) getheaders protocol.
pub fn getheaders_protocol() -> StreamProtocol {
    crate::topics::NetworkTopics::new("dev").getheaders_protocol()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetHeadersRequest {
    /// Newest-first selected-parent spine samples; last entry should be genesis.
    pub locator: Vec<Hash>,
    /// Cap on returned headers (server clamps to [`MAX_HEADERS_PER_RESPONSE`]).
    pub limit: u32,
    /// Optional stop hash (inclusive). `None` means toward the virtual tip.
    pub stop_hash: Option<Hash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetHeadersResponse {
    /// Oldest → newest along the selected-parent chain after the common ancestor.
    /// Empty means the peer is not ahead (or no progress from the locator).
    pub headers: Vec<BlockHeader>,
}

impl GetHeadersRequest {
    pub fn new(locator: Vec<Hash>, limit: u32) -> Self {
        Self {
            locator,
            limit,
            stop_hash: None,
        }
    }
}

impl GetHeadersResponse {
    pub fn empty() -> Self {
        Self {
            headers: Vec::new(),
        }
    }

    pub fn with_headers(headers: Vec<BlockHeader>) -> Self {
        Self { headers }
    }
}

/// Validate that `headers` form a selected-parent spine (oldest → newest):
/// each header (except the first) lists the previous header hash among its parents.
pub fn validate_header_chain(headers: &[BlockHeader]) -> Result<(), String> {
    if headers.is_empty() {
        return Ok(());
    }
    let mut seen = std::collections::HashSet::new();
    for (i, header) in headers.iter().enumerate() {
        let id = header.hash();
        if !seen.insert(id) {
            return Err(format!("duplicate header {}", id.to_hex()));
        }
        if i == 0 {
            continue;
        }
        let parent = headers[i - 1].hash();
        if !header.parents.contains(&parent) {
            return Err(format!(
                "header {} missing selected-parent link to {}",
                id.to_hex(),
                parent.to_hex()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(parents: Vec<Hash>, nonce: u64) -> BlockHeader {
        BlockHeader {
            version: 1,
            parents,
            timestamp_ms: nonce,
            bits: 0,
            nonce,
            tx_root: Hash::ZERO,
        }
    }

    #[test]
    fn validates_linked_spine() {
        let a = hdr(vec![Hash::ZERO], 1);
        let a_id = a.hash();
        let b = hdr(vec![a_id], 2);
        let b_id = b.hash();
        let c = hdr(vec![b_id], 3);
        assert!(validate_header_chain(&[a, b, c]).is_ok());
    }

    #[test]
    fn rejects_broken_link() {
        let a = hdr(vec![Hash::ZERO], 1);
        let b = hdr(vec![Hash::hash_bytes(b"other")], 2);
        assert!(validate_header_chain(&[a, b]).is_err());
    }
}
