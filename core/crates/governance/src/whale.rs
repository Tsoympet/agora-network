use serde::{Deserialize, Serialize};

use crate::GovernanceError;

/// Hard cap on a single voter's countable balance before quadratic transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhaleCapConfig {
    /// Maximum share of total supply countable toward votes, in basis points.
    /// Default: 500 = 5%.
    pub max_share_bps: u64,
}

impl Default for WhaleCapConfig {
    fn default() -> Self {
        Self { max_share_bps: 500 }
    }
}

impl WhaleCapConfig {
    pub const FIVE_PERCENT: Self = Self { max_share_bps: 500 };

    pub fn max_countable_balance(&self, total_supply: u64) -> Result<u64, GovernanceError> {
        if total_supply == 0 {
            return Err(GovernanceError::ZeroSupply);
        }
        Ok(total_supply.saturating_mul(self.max_share_bps) / 10_000)
    }
}

/// Clamp `raw_balance` to the whale hard cap. Returns `(capped, was_capped)`.
pub fn apply_whale_cap(
    raw_balance: u64,
    total_supply: u64,
    config: &WhaleCapConfig,
) -> Result<(u64, bool), GovernanceError> {
    let max = config.max_countable_balance(total_supply)?;
    if raw_balance > max {
        Ok((max, true))
    } else {
        Ok((raw_balance, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_percent_cap_on_supply() {
        let cap = WhaleCapConfig::FIVE_PERCENT;
        let supply = 1_000_000u64;
        assert_eq!(cap.max_countable_balance(supply).unwrap(), 50_000);
        let (capped, hit) = apply_whale_cap(400_000, supply, &cap).unwrap();
        assert!(hit);
        assert_eq!(capped, 50_000);
        let (small, hit) = apply_whale_cap(12_000, supply, &cap).unwrap();
        assert!(!hit);
        assert_eq!(small, 12_000);
    }
}
