use agora_types::{Address, Amount, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum BridgeDirection {
    /// Lock on hub, mint on District.
    LockAndMint,
    /// Burn on District, unlock on hub.
    BurnAndUnlock,
    /// Same-district account payment (XRP Payment–class).
    Payment,
}

/// Canonical cross-domain / payment message.
///
/// `destination_tag` mirrors XRPL destination tags for exchange deposit routing.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub direction: BridgeDirection,
    pub source_district: String,
    pub dest_district: String,
    pub sender: Address,
    pub recipient: Address,
    pub amount: Amount,
    pub nonce: u64,
    /// Exchange / memo routing tag (0 = unused).
    #[serde(default)]
    pub destination_tag: u32,
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
    /// Same-district payment settled.
    Paid,
}
