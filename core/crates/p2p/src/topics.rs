//! Gossipsub topic names — stable identifiers for wire compatibility.

use libp2p::gossipsub::IdentTopic;

pub const TOPIC_BLOCKS: &str = "agora/blocks/1";
pub const TOPIC_TRANSACTIONS: &str = "agora/txs/1";

pub fn blocks_topic() -> IdentTopic {
    IdentTopic::new(TOPIC_BLOCKS)
}

pub fn transactions_topic() -> IdentTopic {
    IdentTopic::new(TOPIC_TRANSACTIONS)
}
