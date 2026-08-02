use agora_types::{Address, Amount, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub enum BridgeDirection {
    /// Lock on Agora / L2, mint on District.
    LockAndMint,
    /// Burn on District, unlock on Agora / L2.
    BurnAndUnlock,
}

/// Canonical cross-domain bridge message.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub direction: BridgeDirection,
    pub source_district: String,
    pub dest_district: String,
    pub sender: Address,
    pub recipient: Address,
    pub amount: Amount,
    pub nonce: u64,
}

impl BridgeMessage {
    pub fn id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageStatus {
    Locked,
    Claimed,
    Unlocked,
}
