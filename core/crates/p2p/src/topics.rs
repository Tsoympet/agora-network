//! Gossipsub topic names scoped by [`crate::NetworkConfig::network`].
//!
//! Format: `agora/<network>/blocks/1` and `agora/<network>/txs/1` so dev and
//! testnet meshes never cross-gossip when peers share an underlay.

use libp2p::gossipsub::IdentTopic;
use libp2p::StreamProtocol;

/// Wire version suffix for gossip topics / getblock protocol.
pub const TOPIC_VERSION: &str = "1";

/// Network-scoped gossip + request-response names for one Agora mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkTopics {
    pub network: String,
}

impl NetworkTopics {
    pub fn new(network: impl Into<String>) -> Self {
        let network = sanitize_network(network.into());
        Self { network }
    }

    pub fn blocks_name(&self) -> String {
        format!("agora/{}/blocks/{}", self.network, TOPIC_VERSION)
    }

    pub fn transactions_name(&self) -> String {
        format!("agora/{}/txs/{}", self.network, TOPIC_VERSION)
    }

    pub fn getblock_protocol_name(&self) -> String {
        format!("/agora/{}/getblock/{}", self.network, TOPIC_VERSION)
    }

    pub fn blocks(&self) -> IdentTopic {
        IdentTopic::new(self.blocks_name())
    }

    pub fn transactions(&self) -> IdentTopic {
        IdentTopic::new(self.transactions_name())
    }

    pub fn getblock_protocol(&self) -> StreamProtocol {
        // StreamProtocol requires a 'static str; leak is fine for process lifetime.
        let name = self.getblock_protocol_name();
        StreamProtocol::try_from_owned(name).expect("getblock protocol")
    }
}

fn sanitize_network(raw: String) -> String {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return "dev".into();
    }
    // Keep multiaddr / gossip names simple: [a-z0-9_-]+
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "dev".into()
    } else {
        cleaned
    }
}

/// Legacy unscoped names (pre–Phase 27). Kept for docs / migration notes only.
pub const TOPIC_BLOCKS: &str = "agora/blocks/1";
pub const TOPIC_TRANSACTIONS: &str = "agora/txs/1";

/// Convenience: topics for the default `dev` network.
pub fn blocks_topic() -> IdentTopic {
    NetworkTopics::new("dev").blocks()
}

pub fn transactions_topic() -> IdentTopic {
    NetworkTopics::new("dev").transactions()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_topics_by_network() {
        let dev = NetworkTopics::new("dev");
        let testnet = NetworkTopics::new("testnet");
        assert_eq!(dev.blocks_name(), "agora/dev/blocks/1");
        assert_eq!(dev.transactions_name(), "agora/dev/txs/1");
        assert_eq!(dev.getblock_protocol_name(), "/agora/dev/getblock/1");
        assert_eq!(testnet.blocks_name(), "agora/testnet/blocks/1");
        assert_ne!(dev.blocks_name(), testnet.blocks_name());
        assert_ne!(
            dev.getblock_protocol_name(),
            testnet.getblock_protocol_name()
        );
    }

    #[test]
    fn sanitizes_network_label() {
        assert_eq!(NetworkTopics::new(" TestNet ").network, "testnet");
        assert_eq!(NetworkTopics::new("foo/bar").network, "foo-bar");
        assert_eq!(NetworkTopics::new("").network, "dev");
    }
}
