//! Issuer-scoped trust lines and exact issued-value transfers (not native DRC).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash};

/// Maximum live trust lines per holder account (no XRPL reserve; hard cap).
pub const DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER: usize = 64;
/// Maximum distinct holder lines tracked per issuer for one currency code.
pub const DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER: usize = 256;

pub const DRC_TRUST_LINE_SET_TX_VERSION: u32 = 1;
pub const DRC_TRUST_LINE_SET_TICKET_VERSION: u32 = 2;
pub const DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION: u32 = 1;
pub const DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION: u32 = 2;

pub const DRC_TRUST_LINE_LIVE_STATE_VERSION: u32 = 1;
#[allow(dead_code)]
pub const DRC_TRUST_LINE_RECEIPT_VERSION: u32 = 1;
pub const DRC_ISSUER_LIABILITY_STATE_VERSION: u32 = 1;
pub const DRC_ISSUED_TRANSFER_RECEIPT_VERSION: u32 = 2;

#[allow(dead_code)]
pub const DRC_TRUST_LINE_SET_TX_TYPE: &[u8] = b"drc_trust_line_set";
#[allow(dead_code)]
pub const DRC_ISSUED_TRANSFER_TX_TYPE: &[u8] = b"drc_issued_transfer";

pub const DRC_TRUST_LINE_SET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-trust-line-set-v1";
pub const DRC_TRUST_LINE_SET_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-trust-line-set-v2";
pub const DRC_ISSUED_TRANSFER_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-issued-transfer-v1";
pub const DRC_ISSUED_TRANSFER_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-issued-transfer-v2";

/// Fixed-width issued currency code (consensus bytes only; no locale strings).
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
pub struct IssuedCurrencyCode(pub [u8; 20]);

impl IssuedCurrencyCode {
    pub fn validate(&self) -> Result<(), IssuedCurrencyError> {
        if self.0 == [0u8; 20] {
            return Err(IssuedCurrencyError::ZeroCode);
        }
        if is_reserved_native_colliding_code(&self.0) {
            return Err(IssuedCurrencyError::ReservedNativeCollidingCode);
        }
        if is_standard_three_byte(&self.0) {
            for b in &self.0[..3] {
                if !b.is_ascii_uppercase() && !b.is_ascii_digit() {
                    return Err(IssuedCurrencyError::MalformedStandardCode);
                }
            }
            if self.0[3..].iter().any(|b| *b != 0) {
                return Err(IssuedCurrencyError::MalformedStandardCode);
            }
            return Ok(());
        }
        // Non-standard 160-bit codes must not use the legacy 0x00 prefix reserved in XRPL.
        if self.0[0] == 0 {
            return Err(IssuedCurrencyError::MalformedNonStandardCode);
        }
        Ok(())
    }
}

fn is_standard_three_byte(code: &[u8; 20]) -> bool {
    code[0].is_ascii_uppercase() || code[0].is_ascii_digit()
}

fn is_reserved_native_colliding_code(code: &[u8; 20]) -> bool {
    const RESERVED: [&[u8; 3]; 4] = [b"DRC", b"TLT", b"OVL", b"XRP"];
    for tag in RESERVED {
        if code[..3] == tag[..] && code[3..].iter().all(|b| *b == 0) {
            return true;
        }
    }
    false
}

/// Issuer-scoped asset identifier (liability domain; never `NativeAssetId`).
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
pub struct IssuedAssetId {
    pub issuer: Address,
    pub currency: IssuedCurrencyCode,
}

impl IssuedAssetId {
    pub fn validate(&self) -> Result<(), IssuedCurrencyError> {
        if self.issuer == Address::ZERO {
            return Err(IssuedCurrencyError::ZeroIssuer);
        }
        self.currency.validate()
    }

    pub fn asset_key(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Exact issued-value amount (never mixed with native `Amount` in consensus math).
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
pub struct IssuedAmount(pub u64);

impl IssuedAmount {
    pub const ZERO: Self = Self(0);

    pub fn from_units(v: u64) -> Self {
        Self(v)
    }

    pub fn as_units(self) -> u64 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcTrustLineSetTx {
    pub version: u32,
    pub holder: Address,
    pub issuer: Address,
    pub currency: IssuedCurrencyCode,
    pub limit: IssuedAmount,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcTrustLineSetTx {
    pub fn asset_id(&self) -> IssuedAssetId {
        IssuedAssetId {
            issuer: self.issuer,
            currency: self.currency,
        }
    }

    pub fn trust_line_set_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn validate_structure(&self) -> Result<(), DrcTrustLineError> {
        if self.version != DRC_TRUST_LINE_SET_TX_VERSION
            && self.version != DRC_TRUST_LINE_SET_TICKET_VERSION
        {
            return Err(DrcTrustLineError::UnsupportedVersion(self.version));
        }
        if self.holder == Address::ZERO || self.issuer == Address::ZERO {
            return Err(DrcTrustLineError::ZeroAccount);
        }
        if self.holder == self.issuer {
            return Err(DrcTrustLineError::SelfTrustLine);
        }
        self.asset_id()
            .validate()
            .map_err(DrcTrustLineError::Asset)?;
        if self.fee.as_base_units() == 0 {
            return Err(DrcTrustLineError::ZeroFee);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let domain = if self.version >= DRC_TRUST_LINE_SET_TICKET_VERSION {
            DRC_TRUST_LINE_SET_TICKET_SIGNING_DOMAIN
        } else {
            DRC_TRUST_LINE_SET_SIGNING_DOMAIN
        };
        borsh::to_vec(&(
            domain,
            chain_id,
            genesis.as_bytes(),
            self.version,
            &self.holder,
            &self.issuer,
            &self.currency,
            &self.limit,
            &self.fee,
            self.nonce,
            self.account_sequence,
        ))
        .unwrap_or_default()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcIssuedTransferTx {
    pub version: u32,
    pub sender: Address,
    pub recipient: Address,
    pub issuer: Address,
    pub currency: IssuedCurrencyCode,
    pub amount: IssuedAmount,
    pub fee: Amount,
    #[serde(default)]
    pub destination_tag: Option<u32>,
    #[serde(default)]
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcIssuedTransferTx {
    pub fn asset_id(&self) -> IssuedAssetId {
        IssuedAssetId {
            issuer: self.issuer,
            currency: self.currency,
        }
    }

    pub fn issued_transfer_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn validate_structure(&self) -> Result<(), DrcTrustLineError> {
        if self.version != DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION
            && self.version != DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION
        {
            return Err(DrcTrustLineError::UnsupportedVersion(self.version));
        }
        if self.sender == Address::ZERO || self.recipient == Address::ZERO {
            return Err(DrcTrustLineError::ZeroAccount);
        }
        if self.sender == self.recipient {
            return Err(DrcTrustLineError::SelfTransfer);
        }
        self.asset_id()
            .validate()
            .map_err(DrcTrustLineError::Asset)?;
        if !self.amount.is_positive() {
            return Err(DrcTrustLineError::NonPositiveAmount);
        }
        if self.invoice_id != Hash::ZERO {
            return Err(DrcTrustLineError::InvoiceNotSupported);
        }
        if self.fee.as_base_units() == 0 {
            return Err(DrcTrustLineError::ZeroFee);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let domain = if self.version >= DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION {
            DRC_ISSUED_TRANSFER_TICKET_SIGNING_DOMAIN
        } else {
            DRC_ISSUED_TRANSFER_SIGNING_DOMAIN
        };
        borsh::to_vec(&(
            domain,
            chain_id,
            genesis.as_bytes(),
            self.version,
            &self.sender,
            &self.recipient,
            &self.issuer,
            &self.currency,
            &self.amount,
            &self.fee,
            self.destination_tag,
            self.source_tag,
            &self.invoice_id,
            self.nonce,
            self.account_sequence,
        ))
        .unwrap_or_default()
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcTrustLineLive {
    pub version: u32,
    pub holder: Address,
    pub asset: IssuedAssetId,
    pub limit: IssuedAmount,
    pub balance: IssuedAmount,
}

impl DrcTrustLineLive {
    pub fn validate(&self) -> Result<(), DrcTrustLineError> {
        if self.version != DRC_TRUST_LINE_LIVE_STATE_VERSION {
            return Err(DrcTrustLineError::UnsupportedLiveVersion(self.version));
        }
        self.asset.validate().map_err(DrcTrustLineError::Asset)?;
        if self.balance.as_units() > self.limit.as_units() {
            return Err(DrcTrustLineError::BalanceExceedsLimit);
        }
        Ok(())
    }

    pub fn available_limit(&self) -> Result<IssuedAmount, DrcTrustLineError> {
        self.limit
            .as_units()
            .checked_sub(self.balance.as_units())
            .map(IssuedAmount::from_units)
            .ok_or(DrcTrustLineError::BalanceExceedsLimit)
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcIssuerLiability {
    pub version: u32,
    pub asset: IssuedAssetId,
    pub outstanding: IssuedAmount,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcIssuedTransferReceipt {
    pub version: u32,
    pub transfer_tx_id: Hash,
    pub asset: IssuedAssetId,
    pub sender: Address,
    pub recipient: Address,
    pub amount: IssuedAmount,
    pub source_tag: Option<u32>,
    pub destination_tag: Option<u32>,
    pub settlement_blue_score: u64,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IssuedCurrencyError {
    #[error("zero currency code")]
    ZeroCode,
    #[error("currency code collides with reserved native marker")]
    ReservedNativeCollidingCode,
    #[error("malformed standard three-byte currency code")]
    MalformedStandardCode,
    #[error("malformed non-standard currency code")]
    MalformedNonStandardCode,
    #[error("zero issuer")]
    ZeroIssuer,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum DrcTrustLineError {
    #[error("unsupported version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported live state version {0}")]
    UnsupportedLiveVersion(u32),
    #[error("asset: {0}")]
    Asset(IssuedCurrencyError),
    #[error("zero account")]
    ZeroAccount,
    #[error("self trust line")]
    SelfTrustLine,
    #[error("self transfer")]
    SelfTransfer,
    #[error("non-positive amount")]
    NonPositiveAmount,
    #[error("invoice routing not supported")]
    InvoiceNotSupported,
    #[error("zero fee")]
    ZeroFee,
    #[error("balance exceeds limit")]
    BalanceExceedsLimit,
}

/// Meta key prefix for live trust line records (matches canonical state layout).
pub const DRC_TRUST_LINE_LINE_META_PREFIX: &[u8] = b"trust/drc/line/";
/// Meta key prefix for issuer outstanding liability.
pub const DRC_TRUST_LINE_LIABILITY_META_PREFIX: &[u8] = b"trust/drc/liability/";

/// Canonical Meta CF key for `(holder, issued asset)` live line state.
pub fn drc_trust_line_live_meta_key(holder: &Address, asset: &IssuedAssetId) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_TRUST_LINE_LINE_META_PREFIX.len() + 20 + 32);
    key.extend_from_slice(DRC_TRUST_LINE_LINE_META_PREFIX);
    key.extend_from_slice(&holder.0);
    key.extend_from_slice(asset.asset_key().as_bytes());
    key
}

/// Canonical Meta CF key for issuer liability on an issued asset.
pub fn drc_trust_line_issuer_liability_meta_key(asset: &IssuedAssetId) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_TRUST_LINE_LIABILITY_META_PREFIX.len() + 32);
    key.extend_from_slice(DRC_TRUST_LINE_LIABILITY_META_PREFIX);
    key.extend_from_slice(asset.asset_key().as_bytes());
    key
}

/// Mempool / admission reservation keys touched by a trust line set (line record only).
pub fn drc_trust_line_set_mutation_meta_keys(tx: &DrcTrustLineSetTx) -> Vec<Vec<u8>> {
    let asset = tx.asset_id();
    vec![drc_trust_line_live_meta_key(&tx.holder, &asset)]
}

/// Mempool / admission reservation keys for source, destination, and liability when changed.
pub fn drc_issued_transfer_mutation_meta_keys(tx: &DrcIssuedTransferTx) -> Vec<Vec<u8>> {
    let asset = tx.asset_id();
    let issuer = asset.issuer;
    let mut keys = vec![drc_trust_line_live_meta_key(&tx.sender, &asset)];
    if tx.recipient != tx.sender {
        keys.push(drc_trust_line_live_meta_key(&tx.recipient, &asset));
    }
    if tx.sender == issuer || tx.recipient == issuer {
        keys.push(drc_trust_line_issuer_liability_meta_key(&asset));
    }
    keys
}

// Borsh for txs with multisign trailer
impl BorshSerialize for DrcTrustLineSetTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.holder, writer)?;
        BorshSerialize::serialize(&self.issuer, writer)?;
        BorshSerialize::serialize(&self.currency, writer)?;
        BorshSerialize::serialize(&self.limit, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcTrustLineSetTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            holder: BorshDeserialize::deserialize_reader(reader)?,
            issuer: BorshDeserialize::deserialize_reader(reader)?,
            currency: BorshDeserialize::deserialize_reader(reader)?,
            limit: BorshDeserialize::deserialize_reader(reader)?,
            fee: BorshDeserialize::deserialize_reader(reader)?,
            nonce: BorshDeserialize::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: BorshDeserialize::deserialize_reader(reader)?,
            signature: BorshDeserialize::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

impl BorshSerialize for DrcIssuedTransferTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.sender, writer)?;
        BorshSerialize::serialize(&self.recipient, writer)?;
        BorshSerialize::serialize(&self.issuer, writer)?;
        BorshSerialize::serialize(&self.currency, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.destination_tag, writer)?;
        BorshSerialize::serialize(&self.source_tag, writer)?;
        BorshSerialize::serialize(&self.invoice_id, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcIssuedTransferTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            sender: BorshDeserialize::deserialize_reader(reader)?,
            recipient: BorshDeserialize::deserialize_reader(reader)?,
            issuer: BorshDeserialize::deserialize_reader(reader)?,
            currency: BorshDeserialize::deserialize_reader(reader)?,
            amount: BorshDeserialize::deserialize_reader(reader)?,
            fee: BorshDeserialize::deserialize_reader(reader)?,
            destination_tag: Option::<u32>::deserialize_reader(reader)?,
            source_tag: Option::<u32>::deserialize_reader(reader)?,
            invoice_id: BorshDeserialize::deserialize_reader(reader)?,
            nonce: BorshDeserialize::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: BorshDeserialize::deserialize_reader(reader)?,
            signature: BorshDeserialize::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

#[cfg(test)]
mod currency_encoding_vectors {
    use super::*;
    use crate::NativeAssetId;
    use borsh::BorshDeserialize;

    #[test]
    fn standard_three_byte_uppercase_and_digit_rules() {
        let ok = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        assert!(ok.validate().is_ok());
        let bad = *b"usd\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        assert!(IssuedCurrencyCode(bad).validate().is_ok());
        let bad_std = *b"U$D\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        assert_eq!(
            IssuedCurrencyCode(bad_std).validate(),
            Err(IssuedCurrencyError::MalformedStandardCode)
        );
        let mut trailing = *b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        trailing[4] = 1;
        assert_eq!(
            IssuedCurrencyCode(trailing).validate(),
            Err(IssuedCurrencyError::MalformedStandardCode)
        );
    }

    #[test]
    fn nonstandard_rejects_legacy_zero_prefix() {
        let mut code = [0u8; 20];
        code[1] = 1;
        assert_eq!(
            IssuedCurrencyCode(code).validate(),
            Err(IssuedCurrencyError::MalformedNonStandardCode)
        );
        code[0] = 0x01;
        assert!(IssuedCurrencyCode(code).validate().is_ok());
    }

    #[test]
    fn canonical_borsh_order_and_asset_key_stability() {
        let issuer = Address([7; 20]);
        let currency = IssuedCurrencyCode(*b"ABC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        let asset = IssuedAssetId { issuer, currency };
        let bytes = borsh::to_vec(&asset).unwrap();
        let decoded = IssuedAssetId::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, asset);
        assert_eq!(asset.asset_key(), asset.asset_key());
    }

    #[test]
    fn issued_asset_id_has_no_native_asset_id_conversion_in_types() {
        fn assert_distinct<T, U>() {}
        assert_distinct::<IssuedAssetId, NativeAssetId>();
    }
}
