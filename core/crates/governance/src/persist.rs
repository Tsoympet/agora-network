//! Durable civic snapshot (governance + community) for node Meta CF.

use serde::{Deserialize, Serialize};

use crate::community::CommunityBoard;
use crate::engine::GovernanceState;

/// JSON blob stored at `meta/governance` (and optionally mirrored in files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivicSnapshot {
    pub version: u32,
    pub governance: GovernanceState,
    pub community: CommunityBoard,
}

impl CivicSnapshot {
    pub const VERSION: u32 = 1;

    pub fn genesis(eligible_power: u64) -> Self {
        Self {
            version: Self::VERSION,
            governance: GovernanceState::genesis(eligible_power),
            community: CommunityBoard::default(),
        }
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Meta CF key for the civic snapshot.
pub const CIVIC_META_KEY: &[u8] = b"meta/governance";
