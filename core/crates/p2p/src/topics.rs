//! Gossipsub topic names bound to the full network fingerprint.

use agora_types::NetworkFingerprint;
use libp2p::gossipsub::IdentTopic;

/// Legacy constant topic names — prefer fingerprint-bound helpers below.
pub const TOPIC_BLOCKS: &str = "agora/blocks/1";
pub const TOPIC_TRANSACTIONS: &str = "agora/txs/1";

/// Blocks gossip topic for a specific network fingerprint.
pub fn blocks_topic(fingerprint: &NetworkFingerprint) -> IdentTopic {
    IdentTopic::new(format!("agora/{}/blocks/1", fingerprint.digest_hex()))
}

/// Transactions gossip topic for a specific network fingerprint.
pub fn transactions_topic(fingerprint: &NetworkFingerprint) -> IdentTopic {
    IdentTopic::new(format!("agora/{}/txs/1", fingerprint.digest_hex()))
}
