use agora_types::{Address, Amount, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Declarative user intent for cross-domain asset orchestration.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Intent {
    pub id_salt: u64,
    pub user: Address,
    pub give_asset_district: String,
    pub give_amount: Amount,
    pub want_asset_district: String,
    pub min_receive: Amount,
    pub deadline_ms: u64,
    /// Optional solver hint (model / strategy id) — non-consensus metadata.
    pub solver_hint: String,
}

impl Intent {
    pub fn id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms > self.deadline_ms
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntentStatus {
    Open,
    Routed,
    Settled,
    Failed,
    Cancelled,
}

/// Concrete route produced by a solver.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Solution {
    pub intent_id: Hash,
    pub receive_amount: Amount,
    pub route: Vec<String>,
    /// `amm` | `bridge` | `composite`
    pub strategy: String,
}
