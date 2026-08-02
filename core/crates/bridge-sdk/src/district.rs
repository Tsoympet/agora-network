use serde::{Deserialize, Serialize};

/// Privacy / domain profile for a District Chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistrictKind {
    Gaming,
    Privacy,
    General,
}

/// Configuration for a custom District Chain bridged to Agora L1 / Ovolos L2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistrictConfig {
    pub district_id: String,
    pub kind: DistrictKind,
    pub chain_id: u64,
    /// Human-readable endpoint / RPC hint for operators (not consensus-critical).
    pub rpc_hint: String,
}

impl DistrictConfig {
    pub fn gaming(district_id: impl Into<String>, chain_id: u64) -> Self {
        Self {
            district_id: district_id.into(),
            kind: DistrictKind::Gaming,
            chain_id,
            rpc_hint: String::new(),
        }
    }

    pub fn privacy(district_id: impl Into<String>, chain_id: u64) -> Self {
        Self {
            district_id: district_id.into(),
            kind: DistrictKind::Privacy,
            chain_id,
            rpc_hint: String::new(),
        }
    }
}
