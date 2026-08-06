//! Protocol-native asset identifiers for Agora Trident L1.
//!
//! Wire bytes are stable: `0x00` TLT, `0x01` OVL, `0x02` DRC. Every native value
//! entry must identify its asset — consensus must never infer asset from context alone.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Amount;

/// Canonical native asset on Agora Trident L1.
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
pub enum NativeAssetId {
    /// Talanton — only mineable asset (RandomX).
    TLT = 0x00,
    /// Ovolos — execution / technical validator asset (never mined).
    OVL = 0x01,
    /// Drachma — payments / community validator asset (never mined).
    DRC = 0x02,
}

impl NativeAssetId {
    pub const ALL: [Self; 3] = [Self::TLT, Self::OVL, Self::DRC];

    pub const fn wire_byte(self) -> u8 {
        self as u8
    }

    pub const fn from_wire_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::TLT),
            0x01 => Some(Self::OVL),
            0x02 => Some(Self::DRC),
            _ => None,
        }
    }

    pub const fn ticker(self) -> &'static str {
        match self {
            Self::TLT => "TLT",
            Self::OVL => "OVL",
            Self::DRC => "DRC",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::TLT => "Talanton",
            Self::OVL => "Ovolos",
            Self::DRC => "Drachma",
        }
    }

    /// Only TLT may be issued by PoW block production.
    pub const fn is_mineable(self) -> bool {
        matches!(self, Self::TLT)
    }
}

impl std::fmt::Display for NativeAssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.ticker())
    }
}

/// Asset-tagged amount. Arithmetic requires matching assets.
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
pub struct NativeAmount {
    pub asset: NativeAssetId,
    pub value: Amount,
}

impl NativeAmount {
    pub const fn new(asset: NativeAssetId, value: Amount) -> Self {
        Self { asset, value }
    }

    pub const fn zero(asset: NativeAssetId) -> Self {
        Self {
            asset,
            value: Amount::ZERO,
        }
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        if self.asset != rhs.asset {
            return None;
        }
        Some(Self {
            asset: self.asset,
            value: self.value.checked_add(rhs.value)?,
        })
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        if self.asset != rhs.asset {
            return None;
        }
        Some(Self {
            asset: self.asset,
            value: self.value.checked_sub(rhs.value)?,
        })
    }
}

/// Trident UTXO/account output shape (asset-explicit).
///
/// Existing v2 `TxOut` remains TLT-implicit for the frozen testnet wire format.
/// Phase 2 state transition consumes this type (or a versioned transaction body).
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
pub struct AssetTxOut {
    pub asset: NativeAssetId,
    pub value: Amount,
    pub address: crate::Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    #[test]
    fn wire_bytes_stable() {
        assert_eq!(NativeAssetId::TLT.wire_byte(), 0x00);
        assert_eq!(NativeAssetId::OVL.wire_byte(), 0x01);
        assert_eq!(NativeAssetId::DRC.wire_byte(), 0x02);
        assert_eq!(NativeAssetId::from_wire_byte(0x01), Some(NativeAssetId::OVL));
        assert_eq!(NativeAssetId::from_wire_byte(0x03), None);
        assert!(NativeAssetId::TLT.is_mineable());
        assert!(!NativeAssetId::OVL.is_mineable());
        assert!(!NativeAssetId::DRC.is_mineable());
        // Borsh discriminant must match the stable wire byte.
        assert_eq!(borsh::to_vec(&NativeAssetId::TLT).unwrap(), vec![0x00]);
        assert_eq!(borsh::to_vec(&NativeAssetId::OVL).unwrap(), vec![0x01]);
        assert_eq!(borsh::to_vec(&NativeAssetId::DRC).unwrap(), vec![0x02]);
    }

    #[test]
    fn native_amount_rejects_cross_asset_math() {
        let tlt = NativeAmount::new(NativeAssetId::TLT, Amount::from_base_units(10));
        let ovl = NativeAmount::new(NativeAssetId::OVL, Amount::from_base_units(10));
        assert!(tlt.checked_add(ovl).is_none());
        assert!(tlt.checked_sub(ovl).is_none());
        let sum = tlt
            .checked_add(NativeAmount::new(
                NativeAssetId::TLT,
                Amount::from_base_units(5),
            ))
            .unwrap();
        assert_eq!(sum.value.as_base_units(), 15);
    }

    #[test]
    fn borsh_roundtrip_asset_types() {
        let amt = NativeAmount::new(NativeAssetId::DRC, Amount::from_base_units(42));
        let bytes = borsh::to_vec(&amt).unwrap();
        let decoded = NativeAmount::try_from_slice(&bytes).unwrap();
        assert_eq!(amt, decoded);

        let out = AssetTxOut {
            asset: NativeAssetId::OVL,
            value: Amount::from_base_units(7),
            address: crate::Address::ZERO,
        };
        let bytes = borsh::to_vec(&out).unwrap();
        let decoded = AssetTxOut::try_from_slice(&bytes).unwrap();
        assert_eq!(out, decoded);
    }
}
