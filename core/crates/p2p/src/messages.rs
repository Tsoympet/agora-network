use agora_types::{Block, Hash, Transaction};
use borsh::{BorshDeserialize, BorshSerialize};

/// Wire envelopes for gossip payloads.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NetworkMessage {
    Transaction(Transaction),
    Block(Block),
    /// Compact announcement used before full block fetch (IBD follow-on).
    BlockAnnounce {
        hash: Hash,
    },
}

impl NetworkMessage {
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("network message borsh encode")
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, borsh::io::Error> {
        borsh::from_slice(bytes)
    }
}
