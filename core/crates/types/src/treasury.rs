//! Canonical protocol treasury identifiers and asset-tagged balances.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Amount, Hash, NativeAssetId};

/// Domain separator for network-bound protocol treasury spends.
pub const TREASURY_DISBURSEMENT_DOMAIN: &[u8] = b"agora-treasury-disbursement-v1";

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

/// Controller-signed spend from one protocol treasury. Does not mint.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct TreasuryDisbursement {
    pub version: u32,
    pub treasury: TreasuryId,
    pub beneficiary: Address,
    pub amount: Amount,
    pub reason_hash: Hash,
    pub authorization_root: Hash,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl TreasuryDisbursement {
    pub fn unsigned(
        treasury: TreasuryId,
        beneficiary: Address,
        amount: Amount,
        reason_hash: Hash,
        authorization_root: Hash,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            treasury,
            beneficiary,
            amount,
            reason_hash,
            authorization_root,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            TREASURY_DISBURSEMENT_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.treasury,
            self.beneficiary,
            self.amount,
            self.reason_hash,
            self.authorization_root,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize treasury disbursement body")
    }

    pub fn disbursement_id(&self) -> Hash {
        Hash::hash_borsh(self)
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

    #[test]
    fn disbursement_id_covers_signature_and_chain() {
        let mut spend = TreasuryDisbursement::unsigned(
            TreasuryId::OvlBuilder,
            Address([1; 20]),
            Amount::from_base_units(10),
            Hash([2; 32]),
            Hash([3; 32]),
            0,
        );
        let unsigned = spend.disbursement_id();
        spend.signature = vec![9; 64];
        assert_ne!(spend.disbursement_id(), unsigned);
        assert_ne!(
            spend.signing_bytes_bound("agora-dev", &Hash::ZERO),
            spend.signing_bytes_bound("agora-testnet", &Hash::ZERO)
        );
    }

    #[test]
    fn disbursement_signing_bytes_lock_light_client_vector() {
        let spend = TreasuryDisbursement::unsigned(
            TreasuryId::OvlBuilder,
            Address([2; 20]),
            Amount::from_base_units(10),
            Hash([3; 32]),
            Hash([4; 32]),
            0,
        );
        let genesis = Hash([
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
            0x89, 0xab, 0xcd, 0xef,
        ]);
        let bytes = spend.signing_bytes_bound("agora-testnet-1", &genesis);
        assert_eq!(
            hex::encode(&bytes),
            "1e00000061676f72612d74726561737572792d64697362757273656d656e742d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef010000000102020202020202020202020202020202020202020a00000000000000030303030303030303030303030303030303030303030303030303030303030304040404040404040404040404040404040404040404040404040404040404040000000000000000"
        );
    }
}
