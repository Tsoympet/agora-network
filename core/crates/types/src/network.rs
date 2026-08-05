use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Hash;

/// Full network identity used to bind datadirs, signatures, and gossip topics.
///
/// Two nodes that disagree on any field are on different networks and must not
/// share UTXO state or accept each other's transaction signatures.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct NetworkFingerprint {
    /// Stable network label (e.g. `agora-devnet`).
    pub network_name: String,
    /// Numeric magic / network id.
    pub network_id: u32,
    /// Genesis block hash for this network.
    pub genesis_hash: Hash,
    /// GHOSTDAG `k` parameter.
    pub ghostdag_k: u32,
    /// Absolute max supply in base units.
    pub max_supply: u64,
    /// Premine allocation in base units.
    pub premine: u64,
    /// Initial block subsidy in base units.
    pub initial_reward: u64,
    /// Halving interval in blue-score units.
    pub halving_interval: u64,
}

impl NetworkFingerprint {
    /// Compact digest used as a domain separator and datadir key.
    pub fn digest(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    /// Hex encoding of [`Self::digest`] for paths / topic suffixes.
    pub fn digest_hex(&self) -> String {
        self.digest().to_hex()
    }
}
