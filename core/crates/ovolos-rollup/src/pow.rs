//! Native OVL proof-of-work for L2 rollup blocks.
//!
//! OVL is the native money of the Ovolos layer. Blocks are sealed with
//! SHA-256 leading-zero PoW (portable; L1 remains RandomX for TLT).

use agora_types::{Address, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::Batch;
use crate::RollupError;

/// PoW algorithm id frozen in Ovolos genesis.
pub const OVOLOS_POW_ALGORITHM: &str = "sha256_leading_zero";

/// PoW header committing to a sequenced batch + OVL coinbase.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct OvlBlockHeader {
    pub height: u64,
    pub prev_block_hash: Hash,
    pub batch_id: Hash,
    pub timestamp_ms: u64,
    /// Leading-zero requirement (same convention as L1 `bits`).
    pub bits: u32,
    pub nonce: u64,
    pub miner: Address,
    /// Native OVL coinbase reward in base units.
    pub reward: u64,
}

impl OvlBlockHeader {
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

/// Mined L2 block: PoW header + batch payload.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct OvlBlock {
    pub header: OvlBlockHeader,
    pub batch: Batch,
}

impl OvlBlock {
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

pub fn verify_pow(header: &OvlBlockHeader) -> Result<Hash, RollupError> {
    let hash = header.pow_hash();
    if leading_zero_bits(&hash) < header.bits {
        return Err(RollupError::Execution(format!(
            "OVL PoW insufficient: need {} leading zeros",
            header.bits
        )));
    }
    Ok(hash)
}

/// Mine an OVL block by incrementing nonce until PoW meets `bits`.
pub fn mine_ovl_block(
    mut header: OvlBlockHeader,
    batch: Batch,
    max_nonces: u64,
) -> Result<OvlBlock, RollupError> {
    if batch.id() != header.batch_id {
        return Err(RollupError::Execution(
            "header batch_id does not match batch".into(),
        ));
    }
    for n in 0..max_nonces {
        header.nonce = n;
        if leading_zero_bits(&header.pow_hash()) >= header.bits {
            return Ok(OvlBlock { header, batch });
        }
    }
    Err(RollupError::Execution(
        "OVL mine: nonce space exhausted".into(),
    ))
}

/// Bitcoin-shaped OVL emission schedule (base units).
#[derive(Debug, Clone)]
pub struct OvlEmission {
    pub initial_reward: u64,
    pub halving_interval: u64,
}

impl Default for OvlEmission {
    fn default() -> Self {
        Self {
            // 50 OVL
            initial_reward: 5_000_000_000,
            halving_interval: 210_000,
        }
    }
}

impl OvlEmission {
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
    use agora_types::Address;

    #[test]
    fn mine_and_verify_bits0() {
        let batch = Batch {
            sequence: 0,
            prev_state_root: Hash::ZERO,
            post_state_root: Hash([1u8; 32]),
            transactions: vec![],
            posted_at_ms: 1,
        };
        let header = OvlBlockHeader {
            height: 0,
            prev_block_hash: Hash::ZERO,
            batch_id: batch.id(),
            timestamp_ms: 1,
            bits: 0,
            nonce: 0,
            miner: Address([9u8; 20]),
            reward: 50,
        };
        let block = mine_ovl_block(header, batch, 1).unwrap();
        verify_pow(&block.header).unwrap();
    }
}
