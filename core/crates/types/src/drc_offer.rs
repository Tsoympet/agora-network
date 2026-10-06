//! Protocol-native DRC order book (OfferCreate / OfferCancel).
//!
//! Amounts and quality are integers. Quality is compared by cross-multiplication
//! in `u128`. This is an Agora semantic adaptation of an order book, not XRPL
//! wire, reserve, or API parity. Path payments, autobridging, rippling, transfer
//! rates, and AMMs are not part of this type family.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash, IssuedAssetId};

/// Hard owner cap. Agora does not copy XRP owner reserves.
pub const DRC_MAX_LIVE_OFFERS_PER_ACCOUNT: usize = 32;
/// Hard directory cap for one directed book.
pub const DRC_MAX_OFFERS_PER_BOOK: usize = 256;
/// Crossing steps (fills, self-cancels, unfunded removals) allowed in one transaction.
pub const DRC_MAX_OFFER_MATCHES_PER_TX: usize = 16;
/// Crossing steps allowed across one block's OfferCreate lane.
pub const DRC_MAX_OFFER_MATCHES_PER_BLOCK: usize = 64;
pub const DRC_OFFER_PAGE_MAX: usize = 32;

pub const DRC_OFFER_CREATE_TX_VERSION: u32 = 1;
pub const DRC_OFFER_CREATE_TICKET_VERSION: u32 = 2;
pub const DRC_OFFER_CANCEL_TX_VERSION: u32 = 1;
pub const DRC_OFFER_CANCEL_TICKET_VERSION: u32 = 2;
pub const DRC_OFFER_LIVE_STATE_VERSION: u32 = 1;
pub const DRC_OFFER_CREATE_RECEIPT_VERSION: u32 = 1;
pub const DRC_OFFER_CANCEL_RECEIPT_VERSION: u32 = 1;

pub const DRC_OFFER_CREATE_TX_TYPE: &[u8] = b"drc_offer_create";
pub const DRC_OFFER_CANCEL_TX_TYPE: &[u8] = b"drc_offer_cancel";

pub const DRC_OFFER_CREATE_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-offer-create-v1";
pub const DRC_OFFER_CREATE_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-offer-create-v2";
pub const DRC_OFFER_CANCEL_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-offer-cancel-v1";
pub const DRC_OFFER_CANCEL_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-offer-cancel-v2";

pub const DRC_OFFER_MAX_BLUE_SCORE_BOUND: u64 = u64::MAX - 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcOfferError {
    #[error("unsupported DRC offer version {0}")]
    UnsupportedVersion(u32),
    #[error("ticket selector on legacy DRC offer version")]
    TicketSelectorOnLegacyVersion,
    #[error("zero address")]
    ZeroAddress,
    #[error("offer amount must be positive")]
    ZeroAmount,
    #[error("offer book must exchange two distinct assets")]
    IdenticalAssets,
    #[error("native DRC is not an issued asset")]
    NativeIssuedConfusion,
    #[error("unsupported offer fill mode")]
    BadFillMode,
    #[error("unsupported offer time in force")]
    BadTimeInForce,
    #[error("offer expiration is out of range")]
    ExpirationOutOfRange,
    #[error("malformed live offer")]
    MalformedLive,
}

/// One side of a book. Native DRC is never an [`IssuedAssetId`].
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
#[serde(tag = "type", rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcBookAsset {
    NativeDrc = 1,
    Issued(IssuedAssetId) = 2,
}

impl DrcBookAsset {
    pub fn validate(self) -> Result<(), DrcOfferError> {
        match self {
            Self::NativeDrc => Ok(()),
            Self::Issued(asset) => asset
                .validate()
                .map_err(|_| DrcOfferError::NativeIssuedConfusion),
        }
    }

    pub const fn is_native(self) -> bool {
        matches!(self, Self::NativeDrc)
    }

    pub const fn issued(self) -> Option<IssuedAssetId> {
        match self {
            Self::NativeDrc => None,
            Self::Issued(asset) => Some(asset),
        }
    }
}

/// Directed book: taker pays `pays` and receives `gets` from the resting offer.
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
pub struct DrcOfferBook {
    pub pays: DrcBookAsset,
    pub gets: DrcBookAsset,
}

impl DrcOfferBook {
    pub fn validate(self) -> Result<(), DrcOfferError> {
        self.pays.validate()?;
        self.gets.validate()?;
        if self.pays == self.gets {
            return Err(DrcOfferError::IdenticalAssets);
        }
        Ok(())
    }

    pub const fn opposite(self) -> Self {
        Self {
            pays: self.gets,
            gets: self.pays,
        }
    }

    pub fn book_key(self) -> Hash {
        Hash::hash_borsh(&(b"agora-drc-offer-book-v1", self))
    }
}

/// Buy is complete when the taker has acquired `taker_pays`. Sell is complete
/// when the taker has spent `taker_gets`.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcOfferFillMode {
    Buy = 1,
    Sell = 2,
}

impl DrcOfferFillMode {
    pub fn from_u8(value: u8) -> Result<Self, DrcOfferError> {
        match value {
            1 => Ok(Self::Buy),
            2 => Ok(Self::Sell),
            _ => Err(DrcOfferError::BadFillMode),
        }
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcOfferTimeInForce {
    GoodTillCancel = 1,
    ImmediateOrCancel = 2,
    FillOrKill = 3,
}

impl DrcOfferTimeInForce {
    pub fn from_u8(value: u8) -> Result<Self, DrcOfferError> {
        match value {
            1 => Ok(Self::GoodTillCancel),
            2 => Ok(Self::ImmediateOrCancel),
            3 => Ok(Self::FillOrKill),
            _ => Err(DrcOfferError::BadTimeInForce),
        }
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcOfferCancelOutcome {
    Removed = 1,
    Absent = 2,
}

/// `true` when `gets_a / pays_a` is strictly greater than `gets_b / pays_b`.
pub fn offer_quality_better(gets_a: u64, pays_a: u64, gets_b: u64, pays_b: u64) -> bool {
    u128::from(gets_a) * u128::from(pays_b) > u128::from(gets_b) * u128::from(pays_a)
}

/// Resting offers cross when the taker can meet the maker's rate.
///
/// `taker_gets` is what the new offer sells and `taker_pays` is what it buys.
/// `maker_gets` / `maker_pays` use the resting offer's own fields (the opposite book).
pub fn offers_cross(taker_gets: u64, taker_pays: u64, maker_gets: u64, maker_pays: u64) -> bool {
    if taker_gets == 0 || taker_pays == 0 || maker_gets == 0 || maker_pays == 0 {
        return false;
    }
    u128::from(taker_gets) * u128::from(maker_gets)
        >= u128::from(maker_pays) * u128::from(taker_pays)
}

/// Consume a maker at its own rate. Input is rounded up so the taker never
/// pays less than the maker's rate. `None` is a dust step.
pub fn offer_fill_step(
    maker_gets: u64,
    maker_pays: u64,
    want_out: u64,
    budget_in: u64,
) -> Option<(u64, u64)> {
    if maker_gets == 0 || maker_pays == 0 || want_out == 0 || budget_in == 0 {
        return None;
    }
    let gets = u128::from(maker_gets);
    let pays = u128::from(maker_pays);
    let want = u128::from(want_out).min(gets);
    let budget = u128::from(budget_in).min(pays);
    let out = want.min(budget * gets / pays);
    if out == 0 {
        return None;
    }
    let input = (out * pays).div_ceil(gets);
    if input == 0 || input > budget || input > pays {
        return None;
    }
    Some((out as u64, input as u64))
}

fn validate_expiration(bound: Option<u64>) -> Result<(), DrcOfferError> {
    if bound.is_some_and(|score| score == 0 || score > DRC_OFFER_MAX_BLUE_SCORE_BOUND) {
        return Err(DrcOfferError::ExpirationOutOfRange);
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcOfferCreateTx {
    pub version: u32,
    pub owner: Address,
    pub taker_pays: DrcBookAsset,
    pub taker_pays_amount: u64,
    pub taker_gets: DrcBookAsset,
    pub taker_gets_amount: u64,
    pub fill_mode: u8,
    pub time_in_force: u8,
    pub fee: Amount,
    #[serde(default)]
    pub expires_after_blue_score: Option<u64>,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcOfferCreateTx {
    pub fn book(&self) -> DrcOfferBook {
        DrcOfferBook {
            pays: self.taker_pays,
            gets: self.taker_gets,
        }
    }

    pub fn validate_structure(&self) -> Result<(), DrcOfferError> {
        if self.version != DRC_OFFER_CREATE_TX_VERSION
            && self.version != DRC_OFFER_CREATE_TICKET_VERSION
        {
            return Err(DrcOfferError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_OFFER_CREATE_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcOfferError::TicketSelectorOnLegacyVersion);
        }
        if self.owner == Address::ZERO {
            return Err(DrcOfferError::ZeroAddress);
        }
        if self.taker_pays_amount == 0 || self.taker_gets_amount == 0 {
            return Err(DrcOfferError::ZeroAmount);
        }
        self.book().validate()?;
        DrcOfferFillMode::from_u8(self.fill_mode)?;
        DrcOfferTimeInForce::from_u8(self.time_in_force)?;
        validate_expiration(self.expires_after_blue_score)?;
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_OFFER_CREATE_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("offer create v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_OFFER_CREATE_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_OFFER_CREATE_TX_TYPE,
                self.version,
                self.owner,
                self.taker_pays,
                self.taker_pays_amount,
                self.taker_gets,
                self.taker_gets_amount,
                self.fill_mode,
                self.time_in_force,
                self.fee,
                self.expires_after_blue_score,
                sequence,
            ))
            .expect("borsh offer create v2");
        }
        borsh::to_vec(&(
            DRC_OFFER_CREATE_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_OFFER_CREATE_TX_TYPE,
            self.version,
            self.owner,
            self.taker_pays,
            self.taker_pays_amount,
            self.taker_gets,
            self.taker_gets_amount,
            self.fill_mode,
            self.time_in_force,
            self.fee,
            self.expires_after_blue_score,
            self.nonce,
        ))
        .expect("borsh offer create v1")
    }

    pub fn offer_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcOfferCreateTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.taker_pays, writer)?;
        BorshSerialize::serialize(&self.taker_pays_amount, writer)?;
        BorshSerialize::serialize(&self.taker_gets, writer)?;
        BorshSerialize::serialize(&self.taker_gets_amount, writer)?;
        BorshSerialize::serialize(&self.fill_mode, writer)?;
        BorshSerialize::serialize(&self.time_in_force, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.expires_after_blue_score, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcOfferCreateTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            owner: Address::deserialize_reader(reader)?,
            taker_pays: DrcBookAsset::deserialize_reader(reader)?,
            taker_pays_amount: u64::deserialize_reader(reader)?,
            taker_gets: DrcBookAsset::deserialize_reader(reader)?,
            taker_gets_amount: u64::deserialize_reader(reader)?,
            fill_mode: u8::deserialize_reader(reader)?,
            time_in_force: u8::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            expires_after_blue_score: Option::<u64>::deserialize_reader(reader)?,
            nonce: u64::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcOfferCancelTx {
    pub version: u32,
    pub submitter: Address,
    pub offer_id: Hash,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcOfferCancelTx {
    pub fn validate_structure(&self) -> Result<(), DrcOfferError> {
        if self.version != DRC_OFFER_CANCEL_TX_VERSION
            && self.version != DRC_OFFER_CANCEL_TICKET_VERSION
        {
            return Err(DrcOfferError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_OFFER_CANCEL_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcOfferError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.offer_id == Hash::ZERO {
            return Err(DrcOfferError::ZeroAddress);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_OFFER_CANCEL_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("offer cancel v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_OFFER_CANCEL_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_OFFER_CANCEL_TX_TYPE,
                self.version,
                self.submitter,
                self.offer_id,
                self.fee,
                sequence,
            ))
            .expect("borsh offer cancel v2");
        }
        borsh::to_vec(&(
            DRC_OFFER_CANCEL_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_OFFER_CANCEL_TX_TYPE,
            self.version,
            self.submitter,
            self.offer_id,
            self.fee,
            self.nonce,
        ))
        .expect("borsh offer cancel v1")
    }

    pub fn cancel_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcOfferCancelTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.offer_id, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcOfferCancelTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            offer_id: Hash::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            nonce: u64::deserialize_reader(reader)?,
            account_sequence: Option::<DrcAccountSequenceSelector>::deserialize_reader(reader)?,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferLive {
    pub version: u32,
    pub offer_id: Hash,
    pub owner: Address,
    pub taker_pays: DrcBookAsset,
    pub taker_pays_original: u64,
    pub taker_pays_remaining: u64,
    pub taker_gets: DrcBookAsset,
    pub taker_gets_original: u64,
    pub taker_gets_remaining: u64,
    pub fill_mode: u8,
    pub book_sequence: u64,
    pub create_blue_score: u64,
    pub expires_after_blue_score: Option<u64>,
    /// Native DRC locked out of the spendable balance for the remaining sell.
    pub native_locked: u64,
}

impl DrcOfferLive {
    pub fn book(&self) -> DrcOfferBook {
        DrcOfferBook {
            pays: self.taker_pays,
            gets: self.taker_gets,
        }
    }

    pub fn validate(&self) -> Result<(), DrcOfferError> {
        if self.version != DRC_OFFER_LIVE_STATE_VERSION {
            return Err(DrcOfferError::UnsupportedVersion(self.version));
        }
        if self.owner == Address::ZERO || self.offer_id == Hash::ZERO {
            return Err(DrcOfferError::ZeroAddress);
        }
        if self.taker_pays_original == 0
            || self.taker_gets_original == 0
            || self.taker_pays_remaining == 0
            || self.taker_gets_remaining == 0
            || self.taker_pays_remaining > self.taker_pays_original
            || self.taker_gets_remaining > self.taker_gets_original
        {
            return Err(DrcOfferError::MalformedLive);
        }
        self.book().validate()?;
        DrcOfferFillMode::from_u8(self.fill_mode)?;
        validate_expiration(self.expires_after_blue_score)?;
        if self.taker_gets.is_native() {
            if self.native_locked != self.taker_gets_remaining {
                return Err(DrcOfferError::MalformedLive);
            }
        } else if self.native_locked != 0 {
            return Err(DrcOfferError::MalformedLive);
        }
        Ok(())
    }

    pub fn expired_at(&self, blue_score: u64) -> bool {
        self.expires_after_blue_score
            .is_some_and(|bound| blue_score > bound)
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferCreateReceipt {
    pub version: u32,
    pub offer_id: Hash,
    pub owner: Address,
    pub placed: bool,
    pub fully_filled: bool,
    pub taker_paid: u64,
    pub taker_received: u64,
    pub steps: u32,
    pub self_cross_cancelled: u32,
    pub removed_offer_ids: Vec<Hash>,
    pub fee: Amount,
    pub settlement_blue_score: u64,
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferCancelReceipt {
    pub version: u32,
    pub cancel_tx_id: Hash,
    pub offer_id: Hash,
    pub submitter: Address,
    pub outcome: DrcOfferCancelOutcome,
    pub fee: Amount,
    pub settlement_blue_score: u64,
}

/// Read model. `taker_gets_funded` is the currently deliverable remainder, not a simulated fill.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferView {
    pub offer: DrcOfferLive,
    pub taker_gets_funded: u64,
    pub expired: bool,
}

/// Account-offer pagination cursor. It is the last returned offer id, not a fill.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferCursor {
    pub offer_id: Hash,
}

impl DrcOfferCursor {
    pub const fn from_offer_id(offer_id: Hash) -> Self {
        Self { offer_id }
    }
}

/// Book pagination cursor. Quality, sequence, and id are the committed sort key.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferBookCursor {
    pub gets_original: u64,
    pub pays_original: u64,
    pub book_sequence: u64,
    pub offer_id: Hash,
}

impl DrcOfferBookCursor {
    pub const fn from_live(offer: &DrcOfferLive) -> Self {
        Self {
            gets_original: offer.taker_gets_original,
            pays_original: offer.taker_pays_original,
            book_sequence: offer.book_sequence,
            offer_id: offer.offer_id,
        }
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferPage {
    pub version: u32,
    pub offers: Vec<DrcOfferView>,
    pub next_cursor: Option<DrcOfferCursor>,
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcOfferBookPage {
    pub version: u32,
    pub book: DrcOfferBook,
    pub offers: Vec<DrcOfferView>,
    pub next_cursor: Option<DrcOfferBookCursor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_cross_multiply_handles_boundaries() {
        assert!(offer_quality_better(2, 1, 1, 1));
        assert!(!offer_quality_better(1, 1, 2, 1));
        assert!(!offer_quality_better(u64::MAX, u64::MAX, 1, 1));
        assert!(offer_quality_better(u64::MAX, 1, u64::MAX - 1, 1));
        assert!(offers_cross(40, 50, 100, 50));
        assert!(!offers_cross(10, 50, 100, 50));
        assert!(!offers_cross(0, 1, 1, 1));
    }

    #[test]
    fn fill_step_rounds_input_up_and_rejects_dust() {
        assert_eq!(offer_fill_step(100, 50, 100, 50), Some((100, 50)));
        assert_eq!(offer_fill_step(100, 50, 40, 50), Some((40, 20)));
        assert_eq!(offer_fill_step(3, 2, 2, 2), Some((2, 2)));
        assert_eq!(offer_fill_step(100, 3, 1, 1), Some((1, 1)));
        assert_eq!(offer_fill_step(3, 100, 1, 1), None);
        assert_eq!(offer_fill_step(u64::MAX, u64::MAX, 5, 4), Some((4, 4)));
    }

    #[test]
    fn identical_books_reject() {
        let native = DrcBookAsset::NativeDrc;
        assert!(DrcOfferBook {
            pays: native,
            gets: native
        }
        .validate()
        .is_err());
    }
}
