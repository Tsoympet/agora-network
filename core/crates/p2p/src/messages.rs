use agora_types::{Block, BlockHeader, Hash, Transaction};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::ibd::short_ids_for_block;

/// Wire envelopes for gossip payloads.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NetworkMessage {
    Transaction(Transaction),
    Block(Block),
    /// Hash-only tip signal; peers that lack the body issue [`Self::GetBlock`].
    BlockAnnounce {
        hash: Hash,
    },
    /// Header + short tx ids for mempool inflation (BIP152-style scaffold).
    CompactBlock {
        header: BlockHeader,
        short_ids: Vec<[u8; 8]>,
    },
    /// IBD / compact-miss follow-up: request the full block body by hash.
    GetBlock {
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

    /// Build a compact block gossip payload from a full block.
    pub fn compact_from_block(block: &Block) -> Self {
        Self::CompactBlock {
            header: block.header.clone(),
            short_ids: short_ids_for_block(block),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::Hash;

    #[test]
    fn compact_and_get_block_roundtrip() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 9,
            bits: 1,
            nonce: 2,
            tx_root: Hash::ZERO,
        };
        let compact = NetworkMessage::CompactBlock {
            header: header.clone(),
            short_ids: vec![],
        };
        let decoded = NetworkMessage::decode(&compact.encode()).unwrap();
        assert_eq!(decoded, compact);

        let get = NetworkMessage::GetBlock {
            hash: header.hash(),
        };
        assert_eq!(NetworkMessage::decode(&get.encode()).unwrap(), get);
    }
}
