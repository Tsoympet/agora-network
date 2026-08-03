//! Native DRC proof-of-work for L3 hub / district blocks.
//!
//! DRC is the native money of the Drachma bridge layer. Blocks are sealed with
//! SHA-256 leading-zero PoW (portable; L1 remains RandomX for TLT).

use agora_types::{Address, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::BridgeError;

/// PoW algorithm id frozen in Drachma genesis.
pub const DRACHMA_POW_ALGORITHM: &str = "sha256_leading_zero";

/// PoW header for a Drachma district/hub block.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct DrcBlockHeader {
    pub district_id: String,
    pub height: u64,
    pub prev_block_hash: Hash,
    /// Merkle / commitment root over bridge messages in this block.
    pub messages_root: Hash,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub nonce: u64,
    pub miner: Address,
    /// Native DRC coinbase reward in base units.
    pub reward: u64,
}

impl DrcBlockHeader {
    pub fn pow_hash(&self) -> Hash {
        let bytes = borsh::to_vec(self).expect("borsh header");
        let digest = Sha256::digest(&bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Hash(out)
    }

    pub fn id(&self) -> Hash {
        self.pow_hash()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct DrcBlock {
    pub header: DrcBlockHeader,
    pub message_ids: Vec<Hash>,
}

impl DrcBlock {
    pub fn id(&self) -> Hash {
        self.header.id()
    }
}

pub fn leading_zero_bits(hash: &Hash) -> u32 {
    let mut count = 0u32;
    for byte in hash.as_bytes() {
        if *byte == 0 {
            count += 8;
            continue;
        }
        count += byte.leading_zeros();
        break;
    }
    count
}

pub fn messages_root(ids: &[Hash]) -> Hash {
    if ids.is_empty() {
        return Hash::ZERO;
    }
    let mut hasher = Sha256::new();
    for id in ids {
        hasher.update(id.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Hash(out)
}

pub fn verify_pow(header: &DrcBlockHeader) -> Result<Hash, BridgeError> {
    let hash = header.pow_hash();
    if leading_zero_bits(&hash) < header.bits {
        return Err(BridgeError::Constraint(format!(
            "DRC PoW insufficient: need {} leading zeros",
            header.bits
        )));
    }
    Ok(hash)
}

pub fn mine_drc_block(
    mut header: DrcBlockHeader,
    message_ids: Vec<Hash>,
    max_nonces: u64,
) -> Result<DrcBlock, BridgeError> {
    let root = messages_root(&message_ids);
    if root != header.messages_root {
        return Err(BridgeError::Constraint(
            "header messages_root does not match message_ids".into(),
        ));
    }
    for n in 0..max_nonces {
        header.nonce = n;
        if leading_zero_bits(&header.pow_hash()) >= header.bits {
            return Ok(DrcBlock {
                header,
                message_ids,
            });
        }
    }
    Err(BridgeError::Constraint(
        "DRC mine: nonce space exhausted".into(),
    ))
}

#[derive(Debug, Clone)]
pub struct DrcEmission {
    pub initial_reward: u64,
    pub halving_interval: u64,
}

impl Default for DrcEmission {
    fn default() -> Self {
        Self {
            // 50 DRC
            initial_reward: 5_000_000_000,
            halving_interval: 210_000,
        }
    }
}

impl DrcEmission {
    pub fn reward_at_height(&self, height: u64) -> u64 {
        let halvings = height / self.halving_interval;
        if halvings >= 64 {
            return 0;
        }
        self.initial_reward >> halvings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mine_and_verify_bits0() {
        let ids = vec![Hash([2u8; 32])];
        let header = DrcBlockHeader {
            district_id: "agora-hub".into(),
            height: 0,
            prev_block_hash: Hash::ZERO,
            messages_root: messages_root(&ids),
            timestamp_ms: 1,
            bits: 0,
            nonce: 0,
            miner: Address([3u8; 20]),
            reward: 50,
        };
        let block = mine_drc_block(header, ids, 1).unwrap();
        verify_pow(&block.header).unwrap();
    }
}
