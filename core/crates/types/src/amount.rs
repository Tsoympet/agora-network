use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Base units of AGORA (8 decimal places). Prefer this over raw `u64` at API boundaries.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
pub struct Amount(pub u64);

impl Amount {
    pub const DECIMALS: u32 = 8;
    pub const ZERO: Self = Self(0);

    pub const fn from_base_units(units: u64) -> Self {
        Self(units)
    }

    pub const fn as_base_units(self) -> u64 {
        self.0
    }

    /// Convert whole tokens to base units (truncates fractional input at call site).
    pub fn from_whole(whole: u64) -> Option<Self> {
        whole.checked_mul(10u64.pow(Self::DECIMALS)).map(Self)
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }
}
