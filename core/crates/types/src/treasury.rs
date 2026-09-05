//! Canonical protocol treasury identifiers and asset-tagged balances.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Amount, NativeAssetId};

/// Protocol treasury with a fixed native-asset denomination.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum TreasuryId {
    TltSecurity = 0x00,
    OvlBuilder = 0x01,
    DrcCommunity = 0x02,
}

impl TreasuryId {
    pub const ALL: [Self; 3] = [Self::TltSecurity, Self::OvlBuilder, Self::DrcCommunity];

    pub const fn wire_byte(self) -> u8 {
        self as u8
    }

    pub const fn asset(self) -> NativeAssetId {
        match self {
            Self::TltSecurity => NativeAssetId::TLT,
            Self::OvlBuilder => NativeAssetId::OVL,
            Self::DrcCommunity => NativeAssetId::DRC,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TltSecurity => "tlt_security",
            Self::OvlBuilder => "ovl_builder",
            Self::DrcCommunity => "drc_community",
        }
    }
}

/// Balance held by a protocol treasury in its fixed native asset.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct TreasuryBalance {
    pub treasury: TreasuryId,
    pub asset: NativeAssetId,
    pub balance: Amount,
}

impl TreasuryBalance {
    pub fn new(
        treasury: TreasuryId,
        asset: NativeAssetId,
        balance: Amount,
    ) -> Result<Self, String> {
        if treasury.asset() != asset {
            return Err(format!(
                "treasury {} requires asset {}, got {}",
                treasury.as_str(),
                treasury.asset(),
                asset
            ));
        }
        Ok(Self {
            treasury,
            asset,
            balance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    #[test]
    fn treasury_wire_bytes_and_assets_are_stable() {
        let expected = [
            (
                TreasuryId::TltSecurity,
                0x00,
                NativeAssetId::TLT,
                "tlt_security",
            ),
            (
                TreasuryId::OvlBuilder,
                0x01,
                NativeAssetId::OVL,
                "ovl_builder",
            ),
            (
                TreasuryId::DrcCommunity,
                0x02,
                NativeAssetId::DRC,
                "drc_community",
            ),
        ];

        for (treasury, wire_byte, asset, name) in expected {
            assert_eq!(treasury.wire_byte(), wire_byte);
            assert_eq!(treasury.asset(), asset);
            assert_eq!(treasury.as_str(), name);
            assert_eq!(borsh::to_vec(&treasury).unwrap(), vec![wire_byte]);
        }
    }

    #[test]
    fn treasury_balance_validates_asset_and_roundtrips() {
        let balance = TreasuryBalance::new(
            TreasuryId::OvlBuilder,
            NativeAssetId::OVL,
            Amount::from_base_units(42),
        )
        .unwrap();
        let bytes = borsh::to_vec(&balance).unwrap();
        assert_eq!(TreasuryBalance::try_from_slice(&bytes).unwrap(), balance);

        let error = TreasuryBalance::new(TreasuryId::OvlBuilder, NativeAssetId::DRC, Amount::ZERO)
            .unwrap_err();
        assert_eq!(error, "treasury ovl_builder requires asset OVL, got DRC");
    }
}
