//! Trident hybrid finality checkpoint types.
//!
//! A checkpoint becomes final only when TLT PoW work **and** independent OVL and
//! DRC ⅔ quorums are all satisfied. No price-oracle stake combining. No admin bypass.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Hash, NativeAssetId};

/// Domain separator for checkpoint attestation signatures.
pub const CHECKPOINT_ATTESTATION_DOMAIN: &[u8] = b"agora-trident-checkpoint-v1";

/// Lifecycle of a dual-PoS / PoW checkpoint.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub enum CheckpointState {
    Proposed,
    PoWAccepted,
    AwaitingOvlQuorum,
    AwaitingDrcQuorum,
    Finalized,
    RevertedOrOrphaned,
}

impl CheckpointState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "Proposed",
            Self::PoWAccepted => "PoWAccepted",
            Self::AwaitingOvlQuorum => "AwaitingOvlQuorum",
            Self::AwaitingDrcQuorum => "AwaitingDrcQuorum",
            Self::Finalized => "Finalized",
            Self::RevertedOrOrphaned => "RevertedOrOrphaned",
        }
    }

    pub const fn is_finalized(self) -> bool {
        matches!(self, Self::Finalized)
    }

    /// Unfinalized tips may still reorg under normal rules.
    pub const fn is_reorgable(self) -> bool {
        !matches!(self, Self::Finalized)
    }
}

/// Consensus-relevant checkpoint identity (domain-separated signing body).
#[derive(
    Clone,
    PartialEq,
    Eq,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub struct CheckpointBody {
    pub chain_id: String,
    pub genesis_hash: Hash,
    pub consensus_policy_hash: Hash,
    pub state_transition_version: String,
    pub blue_score: u64,
    pub block_hash: Hash,
    pub state_root: Hash,
    pub validator_epoch: u64,
}

impl CheckpointBody {
    pub fn signing_bytes(&self) -> Vec<u8> {
        borsh::to_vec(&(CHECKPOINT_ATTESTATION_DOMAIN, self))
            .expect("borsh serialize checkpoint body")
    }

    pub fn checkpoint_id(&self) -> Hash {
        Hash::hash_borsh(&(CHECKPOINT_ATTESTATION_DOMAIN, self))
    }
}

/// One validator's attestation over a checkpoint body.
#[derive(
    Clone,
    PartialEq,
    Eq,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub struct CheckpointAttestation {
    pub body: CheckpointBody,
    /// Which validator set this signature belongs to.
    pub set: NativeAssetId,
    pub validator: Address,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Aggregated certificate (may be partial until both quorums + PoW).
#[derive(
    Clone,
    PartialEq,
    Eq,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub struct FinalityCertificate {
    pub body: CheckpointBody,
    pub state: CheckpointState,
    pub pow_work_met: bool,
    pub ovl_signed_stake: u64,
    pub ovl_active_stake: u64,
    pub drc_signed_stake: u64,
    pub drc_active_stake: u64,
}

impl FinalityCertificate {
    pub fn new(body: CheckpointBody) -> Self {
        Self {
            body,
            state: CheckpointState::Proposed,
            pow_work_met: false,
            ovl_signed_stake: 0,
            ovl_active_stake: 0,
            drc_signed_stake: 0,
            drc_active_stake: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalized_not_reorgable() {
        assert!(CheckpointState::Finalized.is_finalized());
        assert!(!CheckpointState::Finalized.is_reorgable());
        assert!(CheckpointState::PoWAccepted.is_reorgable());
    }
}
