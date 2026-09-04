//! Per-asset monetary policy structures for Agora Trident L1.
//!
//! Caps are working defaults from the current mark registry. A genesis ceremony
//! may revise them only via explicit consensus upgrade / new genesis freeze.

use agora_types::{Amount, NativeAssetId};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Working max supplies (base units, 8 decimals).
pub const TLT_MAX_SUPPLY_BASE: u64 = 10_000_000_000_000_000; // 100M
pub const OVL_MAX_SUPPLY_BASE: u64 = 2_100_000_000_000_000_000; // 21B
pub const DRC_MAX_SUPPLY_BASE: u64 = 600_000_000_000_000_000; // 6B

/// Emission / distribution schedule kind for one native asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmissionKind {
    /// Bitcoin-shaped PoW subsidy (TLT only).
    PowHalving {
        initial_reward: u64,
        halving_interval: u64,
    },
    /// Predetermined staking / community reserve (OVL / DRC).
    StakingReserve { reserve_base_units: u64 },
}

/// Protocol monetary policy for one native asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AssetMonetaryPolicy {
    pub asset: NativeAssetId,
    pub max_supply: u64,
    pub decimals: u8,
    pub mineable: bool,
    pub genesis_allocation: u64,
    pub treasury_allocation: u64,
    pub emission: EmissionKind,
}

impl AssetMonetaryPolicy {
    pub fn tlt_default() -> Self {
        Self {
            asset: NativeAssetId::TLT,
            max_supply: TLT_MAX_SUPPLY_BASE,
            decimals: 8,
            mineable: true,
            // 10% premine working default
            genesis_allocation: TLT_MAX_SUPPLY_BASE / 10,
            treasury_allocation: 0,
            emission: EmissionKind::PowHalving {
                initial_reward: 5_000_000_000,
                halving_interval: 210_000,
            },
        }
    }

    pub fn ovl_default() -> Self {
        Self {
            asset: NativeAssetId::OVL,
            max_supply: OVL_MAX_SUPPLY_BASE,
            decimals: 8,
            mineable: false,
            genesis_allocation: 0,
            treasury_allocation: 0,
            // Reserve left for ceremony; must remain ≤ max_supply - genesis - treasury.
            emission: EmissionKind::StakingReserve {
                reserve_base_units: 0,
            },
        }
    }

    pub fn drc_default() -> Self {
        Self {
            asset: NativeAssetId::DRC,
            max_supply: DRC_MAX_SUPPLY_BASE,
            decimals: 8,
            mineable: false,
            genesis_allocation: 0,
            treasury_allocation: 0,
            emission: EmissionKind::StakingReserve {
                reserve_base_units: 0,
            },
        }
    }

    pub fn validate_caps(&self) -> Result<(), String> {
        if self.decimals != 8 {
            return Err(format!("{} decimals must be 8 for v3", self.asset));
        }
        if self.mineable != self.asset.is_mineable() {
            return Err(format!(
                "{} mineable flag {} disagrees with protocol (expected {})",
                self.asset,
                self.mineable,
                self.asset.is_mineable()
            ));
        }
        if self.asset != NativeAssetId::TLT {
            if let EmissionKind::PowHalving { .. } = self.emission {
                return Err(format!("{} cannot use PoW emission", self.asset));
            }
        }
        let reserved = self
            .genesis_allocation
            .checked_add(self.treasury_allocation)
            .ok_or("allocation overflow")?;
        let emission_reserve = match self.emission {
            EmissionKind::PowHalving { .. } => 0u64,
            EmissionKind::StakingReserve { reserve_base_units } => reserve_base_units,
        };
        let committed = reserved
            .checked_add(emission_reserve)
            .ok_or("emission reserve overflow")?;
        if committed > self.max_supply {
            return Err(format!(
                "{} committed supply {} exceeds max {}",
                self.asset, committed, self.max_supply
            ));
        }
        Ok(())
    }

    pub fn max_supply_amount(&self) -> Amount {
        Amount::from_base_units(self.max_supply)
    }
}

/// Bundle of three native policies committed in genesis v3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TridentMonetaryPolicy {
    pub tlt: AssetMonetaryPolicy,
    pub ovl: AssetMonetaryPolicy,
    pub drc: AssetMonetaryPolicy,
}

impl Default for TridentMonetaryPolicy {
    fn default() -> Self {
        Self {
            tlt: AssetMonetaryPolicy::tlt_default(),
            ovl: AssetMonetaryPolicy::ovl_default(),
            drc: AssetMonetaryPolicy::drc_default(),
        }
    }
}

impl TridentMonetaryPolicy {
    pub fn validate(&self) -> Result<(), String> {
        self.tlt.validate_caps()?;
        self.ovl.validate_caps()?;
        self.drc.validate_caps()?;
        if self.tlt.asset != NativeAssetId::TLT
            || self.ovl.asset != NativeAssetId::OVL
            || self.drc.asset != NativeAssetId::DRC
        {
            return Err("monetary policy asset ids must be TLT/OVL/DRC in order".into());
        }
        Ok(())
    }

    pub fn policy(&self, asset: NativeAssetId) -> &AssetMonetaryPolicy {
        match asset {
            NativeAssetId::TLT => &self.tlt,
            NativeAssetId::OVL => &self.ovl,
            NativeAssetId::DRC => &self.drc,
        }
    }
}

/// Issued vs maximum supply invariant helper (Phase 2 wires live counters).
pub fn issued_within_cap(issued: Amount, max: Amount) -> bool {
    issued.as_base_units() <= max.as_base_units()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policies_valid_and_only_tlt_mineable() {
        let p = TridentMonetaryPolicy::default();
        p.validate().unwrap();
        assert!(p.tlt.mineable);
        assert!(!p.ovl.mineable);
        assert!(!p.drc.mineable);
        assert!(issued_within_cap(
            Amount::from_base_units(p.tlt.genesis_allocation),
            p.tlt.max_supply_amount()
        ));
    }

    #[test]
    fn ovl_rejects_pow_emission() {
        let mut p = AssetMonetaryPolicy::ovl_default();
        p.emission = EmissionKind::PowHalving {
            initial_reward: 1,
            halving_interval: 1,
        };
        assert!(p.validate_caps().is_err());
    }

    #[test]
    fn reserve_cannot_exceed_max() {
        let mut p = AssetMonetaryPolicy::drc_default();
        p.genesis_allocation = DRC_MAX_SUPPLY_BASE;
        p.emission = EmissionKind::StakingReserve {
            reserve_base_units: 1,
        };
        assert!(p.validate_caps().is_err());
    }
}
