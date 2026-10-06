//! Issued-asset policy, line issuer controls, and clawback (not native assets).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::drc_trust_line::{IssuedAmount, IssuedAssetId, IssuedCurrencyError};
use crate::{Address, Amount, Hash};

pub const DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION: u32 = 1;
pub const DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION: u32 = 2;
pub const DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION: u32 = 1;
pub const DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION: u32 = 2;
pub const DRC_ISSUED_CLAWBACK_TX_VERSION: u32 = 1;
pub const DRC_ISSUED_CLAWBACK_TICKET_VERSION: u32 = 2;

pub const DRC_ISSUED_ASSET_POLICY_LIVE_VERSION: u32 = 1;
pub const DRC_ISSUED_ASSET_POLICY_RECEIPT_VERSION: u32 = 1;
pub const DRC_TRUST_LINE_ISSUER_CONTROL_RECEIPT_VERSION: u32 = 1;
pub const DRC_ISSUED_CLAWBACK_RECEIPT_VERSION: u32 = 1;

pub const DRC_ISSUED_ASSET_POLICY_SET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-issued-asset-policy-set-v1";
pub const DRC_ISSUED_ASSET_POLICY_SET_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-issued-asset-policy-set-v2";
pub const DRC_TRUST_LINE_ISSUER_CONTROL_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-trust-line-issuer-control-v1";
pub const DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-trust-line-issuer-control-v2";
pub const DRC_ISSUED_CLAWBACK_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-issued-clawback-v1";
pub const DRC_ISSUED_CLAWBACK_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-issued-clawback-v2";

pub const DRC_ISSUED_ASSET_POLICY_META_PREFIX: &[u8] = b"trust/drc/asset-policy/";

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum DrcIssuedAssetPolicyAction {
    EnableRequireAuth,
    EnableGlobalFreeze,
    ClearGlobalFreeze,
    /// Irreversible; fails if any freeze active or clawback already enabled.
    EnableNoFreeze,
    /// Irreversible; only at zero liability; fails if `no_freeze` set.
    EnableClawback,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcIssuedAssetPolicySetTx {
    pub version: u32,
    pub issuer: Address,
    pub currency: crate::IssuedCurrencyCode,
    pub action: DrcIssuedAssetPolicyAction,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcIssuedAssetPolicySetTx {
    pub fn asset_id(&self) -> IssuedAssetId {
        IssuedAssetId {
            issuer: self.issuer,
            currency: self.currency,
        }
    }

    pub fn policy_set_tx_id(&self) -> Hash {
        Hash::hash_borsh(&(
            b"drc_issued_asset_policy_set",
            self.version,
            &self.issuer,
            &self.currency,
            self.action,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
    }

    pub fn validate_structure(&self) -> Result<(), DrcIssuedControlsError> {
        if self.version != DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION
            && self.version != DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION
        {
            return Err(DrcIssuedControlsError::UnsupportedVersion(self.version));
        }
        if self.issuer == Address::ZERO {
            return Err(DrcIssuedControlsError::ZeroAccount);
        }
        self.currency
            .validate()
            .map_err(DrcIssuedControlsError::Asset)?;
        if self.fee.as_base_units() == 0 {
            return Err(DrcIssuedControlsError::ZeroFee);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let domain = if self.version >= DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION {
            DRC_ISSUED_ASSET_POLICY_SET_TICKET_SIGNING_DOMAIN
        } else {
            DRC_ISSUED_ASSET_POLICY_SET_SIGNING_DOMAIN
        };
        Hash::hash_borsh(&(
            domain,
            chain_id,
            genesis,
            self.version,
            &self.issuer,
            &self.currency,
            self.action,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
        .as_bytes()
        .to_vec()
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum DrcTrustLineIssuerControlAction {
    AuthorizeHolder,
    SetLineFrozen(bool),
    SetLineDeepFrozen(bool),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcTrustLineIssuerControlTx {
    pub version: u32,
    pub issuer: Address,
    pub holder: Address,
    pub currency: crate::IssuedCurrencyCode,
    pub action: DrcTrustLineIssuerControlAction,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcTrustLineIssuerControlTx {
    pub fn asset_id(&self) -> IssuedAssetId {
        IssuedAssetId {
            issuer: self.issuer,
            currency: self.currency,
        }
    }

    pub fn issuer_control_tx_id(&self) -> Hash {
        Hash::hash_borsh(&(
            b"drc_trust_line_issuer_control",
            self.version,
            &self.issuer,
            &self.holder,
            &self.currency,
            self.action,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
    }

    pub fn validate_structure(&self) -> Result<(), DrcIssuedControlsError> {
        if self.version != DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION
            && self.version != DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION
        {
            return Err(DrcIssuedControlsError::UnsupportedVersion(self.version));
        }
        if self.issuer == Address::ZERO || self.holder == Address::ZERO {
            return Err(DrcIssuedControlsError::ZeroAccount);
        }
        if self.issuer == self.holder {
            return Err(DrcIssuedControlsError::SelfLine);
        }
        self.currency
            .validate()
            .map_err(DrcIssuedControlsError::Asset)?;
        if self.fee.as_base_units() == 0 {
            return Err(DrcIssuedControlsError::ZeroFee);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let domain = if self.version >= DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION {
            DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_SIGNING_DOMAIN
        } else {
            DRC_TRUST_LINE_ISSUER_CONTROL_SIGNING_DOMAIN
        };
        Hash::hash_borsh(&(
            domain,
            chain_id,
            genesis,
            self.version,
            &self.issuer,
            &self.holder,
            &self.currency,
            self.action,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
        .as_bytes()
        .to_vec()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcIssuedClawbackTx {
    pub version: u32,
    pub issuer: Address,
    pub holder: Address,
    pub currency: crate::IssuedCurrencyCode,
    pub amount: IssuedAmount,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcIssuedClawbackTx {
    pub fn asset_id(&self) -> IssuedAssetId {
        IssuedAssetId {
            issuer: self.issuer,
            currency: self.currency,
        }
    }

    pub fn clawback_tx_id(&self) -> Hash {
        Hash::hash_borsh(&(
            b"drc_issued_clawback",
            self.version,
            &self.issuer,
            &self.holder,
            &self.currency,
            &self.amount,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
    }

    pub fn validate_structure(&self) -> Result<(), DrcIssuedControlsError> {
        if self.version != DRC_ISSUED_CLAWBACK_TX_VERSION
            && self.version != DRC_ISSUED_CLAWBACK_TICKET_VERSION
        {
            return Err(DrcIssuedControlsError::UnsupportedVersion(self.version));
        }
        if self.issuer == Address::ZERO || self.holder == Address::ZERO {
            return Err(DrcIssuedControlsError::ZeroAccount);
        }
        if self.issuer == self.holder {
            return Err(DrcIssuedControlsError::SelfLine);
        }
        if !self.amount.is_positive() {
            return Err(DrcIssuedControlsError::NonPositiveAmount);
        }
        self.currency
            .validate()
            .map_err(DrcIssuedControlsError::Asset)?;
        if self.fee.as_base_units() == 0 {
            return Err(DrcIssuedControlsError::ZeroFee);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let domain = if self.version >= DRC_ISSUED_CLAWBACK_TICKET_VERSION {
            DRC_ISSUED_CLAWBACK_TICKET_SIGNING_DOMAIN
        } else {
            DRC_ISSUED_CLAWBACK_SIGNING_DOMAIN
        };
        Hash::hash_borsh(&(
            domain,
            chain_id,
            genesis,
            self.version,
            &self.issuer,
            &self.holder,
            &self.currency,
            &self.amount,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
        .as_bytes()
        .to_vec()
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcIssuedAssetPolicyLive {
    pub version: u32,
    pub asset: IssuedAssetId,
    pub require_auth: bool,
    pub global_freeze: bool,
    pub no_freeze: bool,
    pub clawback_enabled: bool,
}

impl DrcIssuedAssetPolicyLive {
    pub fn validate(&self) -> Result<(), DrcIssuedControlsError> {
        if self.version != DRC_ISSUED_ASSET_POLICY_LIVE_VERSION {
            return Err(DrcIssuedControlsError::UnsupportedLiveVersion(self.version));
        }
        self.asset
            .validate()
            .map_err(DrcIssuedControlsError::Asset)?;
        if self.no_freeze && self.global_freeze {
            return Err(DrcIssuedControlsError::InvalidPolicyCombination);
        }
        if self.no_freeze && self.clawback_enabled {
            return Err(DrcIssuedControlsError::InvalidPolicyCombination);
        }
        Ok(())
    }

    pub fn default_for_asset(asset: IssuedAssetId) -> Self {
        Self {
            version: DRC_ISSUED_ASSET_POLICY_LIVE_VERSION,
            asset,
            require_auth: false,
            global_freeze: false,
            no_freeze: false,
            clawback_enabled: false,
        }
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcIssuedAssetPolicyReceipt {
    pub version: u32,
    pub policy_set_tx_id: Hash,
    pub asset: IssuedAssetId,
    pub action: DrcIssuedAssetPolicyAction,
    pub settlement_blue_score: u64,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcTrustLineIssuerControlReceipt {
    pub version: u32,
    pub control_tx_id: Hash,
    pub asset: IssuedAssetId,
    pub holder: Address,
    pub action: DrcTrustLineIssuerControlAction,
    pub settlement_blue_score: u64,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcIssuedClawbackReceipt {
    pub version: u32,
    pub clawback_tx_id: Hash,
    pub asset: IssuedAssetId,
    pub holder: Address,
    pub amount: IssuedAmount,
    pub settlement_blue_score: u64,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum DrcIssuedControlsError {
    #[error("unsupported version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported live version {0}")]
    UnsupportedLiveVersion(u32),
    #[error("asset: {0}")]
    Asset(IssuedCurrencyError),
    #[error("zero account")]
    ZeroAccount,
    #[error("self line")]
    SelfLine,
    #[error("non-positive amount")]
    NonPositiveAmount,
    #[error("zero fee")]
    ZeroFee,
    #[error("invalid policy combination")]
    InvalidPolicyCombination,
}

pub fn drc_issued_asset_policy_meta_key(asset: &IssuedAssetId) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_ISSUED_ASSET_POLICY_META_PREFIX.len() + 32);
    key.extend_from_slice(DRC_ISSUED_ASSET_POLICY_META_PREFIX);
    key.extend_from_slice(asset.asset_key().as_bytes());
    key
}

pub fn drc_issued_asset_policy_set_mutation_meta_keys(
    tx: &DrcIssuedAssetPolicySetTx,
) -> Vec<Vec<u8>> {
    vec![drc_issued_asset_policy_meta_key(&tx.asset_id())]
}

pub fn drc_trust_line_issuer_control_mutation_meta_keys(
    tx: &DrcTrustLineIssuerControlTx,
) -> Vec<Vec<u8>> {
    let asset = tx.asset_id();
    vec![crate::drc_trust_line::drc_trust_line_live_meta_key(
        &tx.holder, &asset,
    )]
}

pub fn drc_issued_clawback_mutation_meta_keys(tx: &DrcIssuedClawbackTx) -> Vec<Vec<u8>> {
    let asset = tx.asset_id();
    vec![
        crate::drc_trust_line::drc_trust_line_live_meta_key(&tx.holder, &asset),
        crate::drc_trust_line::drc_trust_line_issuer_liability_meta_key(&asset),
    ]
}

impl BorshSerialize for DrcIssuedAssetPolicySetTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.issuer, writer)?;
        BorshSerialize::serialize(&self.currency, writer)?;
        BorshSerialize::serialize(&self.action, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcIssuedAssetPolicySetTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            issuer: BorshDeserialize::deserialize_reader(reader)?,
            currency: BorshDeserialize::deserialize_reader(reader)?,
            action: BorshDeserialize::deserialize_reader(reader)?,
            fee: BorshDeserialize::deserialize_reader(reader)?,
            nonce: BorshDeserialize::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: BorshDeserialize::deserialize_reader(reader)?,
            signature: BorshDeserialize::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

impl BorshSerialize for DrcTrustLineIssuerControlTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.issuer, writer)?;
        BorshSerialize::serialize(&self.holder, writer)?;
        BorshSerialize::serialize(&self.currency, writer)?;
        BorshSerialize::serialize(&self.action, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcTrustLineIssuerControlTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            issuer: BorshDeserialize::deserialize_reader(reader)?,
            holder: BorshDeserialize::deserialize_reader(reader)?,
            currency: BorshDeserialize::deserialize_reader(reader)?,
            action: BorshDeserialize::deserialize_reader(reader)?,
            fee: BorshDeserialize::deserialize_reader(reader)?,
            nonce: BorshDeserialize::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: BorshDeserialize::deserialize_reader(reader)?,
            signature: BorshDeserialize::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

impl BorshSerialize for DrcIssuedClawbackTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.issuer, writer)?;
        BorshSerialize::serialize(&self.holder, writer)?;
        BorshSerialize::serialize(&self.currency, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcIssuedClawbackTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            issuer: BorshDeserialize::deserialize_reader(reader)?,
            holder: BorshDeserialize::deserialize_reader(reader)?,
            currency: BorshDeserialize::deserialize_reader(reader)?,
            amount: BorshDeserialize::deserialize_reader(reader)?,
            fee: BorshDeserialize::deserialize_reader(reader)?,
            nonce: BorshDeserialize::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: BorshDeserialize::deserialize_reader(reader)?,
            signature: BorshDeserialize::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

#[cfg(test)]
mod native_asset_barrier_tests {
    use super::*;
    use crate::IssuedCurrencyCode;

    #[test]
    fn policy_set_rejects_native_colliding_currency_code() {
        let mut code = [0u8; 20];
        code[..3].copy_from_slice(b"TLT");
        let currency = IssuedCurrencyCode(code);
        assert!(currency.validate().is_err());
        let tx = DrcIssuedAssetPolicySetTx {
            version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
            issuer: Address([1u8; 20]),
            currency,
            action: DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(tx.validate_structure().is_err());
    }
}
