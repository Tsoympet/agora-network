//! Typed, contract-free native DRC escrow (GHOSTDAG blue-score time gates only).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash};

pub const DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT: usize = 32;

pub const DRC_ESCROW_CREATE_TX_VERSION: u32 = 1;
pub const DRC_ESCROW_CREATE_TICKET_VERSION: u32 = 2;
pub const DRC_ESCROW_FINISH_TX_VERSION: u32 = 1;
pub const DRC_ESCROW_FINISH_TICKET_VERSION: u32 = 2;
pub const DRC_ESCROW_CANCEL_TX_VERSION: u32 = 1;
pub const DRC_ESCROW_CANCEL_TICKET_VERSION: u32 = 2;
pub const DRC_ESCROW_LIVE_STATE_VERSION: u32 = 1;
pub const DRC_ESCROW_RECEIPT_VERSION: u32 = 1;

pub const DRC_ESCROW_CREATE_TX_TYPE: &[u8] = b"drc_escrow_create";
pub const DRC_ESCROW_FINISH_TX_TYPE: &[u8] = b"drc_escrow_finish";
pub const DRC_ESCROW_CANCEL_TX_TYPE: &[u8] = b"drc_escrow_cancel";

pub const DRC_ESCROW_CREATE_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-escrow-create-v1";
pub const DRC_ESCROW_CREATE_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-escrow-create-v2";
pub const DRC_ESCROW_FINISH_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-escrow-finish-v1";
pub const DRC_ESCROW_FINISH_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-escrow-finish-v2";
pub const DRC_ESCROW_CANCEL_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-escrow-cancel-v1";
pub const DRC_ESCROW_CANCEL_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-escrow-cancel-v2";

/// Maximum representable blue score (reject at bounds validation).
pub const DRC_ESCROW_MAX_BLUE_SCORE_BOUND: u64 = u64::MAX - 1;

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcEscrowOutcome {
    Finished = 1,
    Cancelled = 2,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcEscrowCreateTx {
    pub version: u32,
    pub owner: Address,
    pub recipient: Address,
    pub amount: Amount,
    pub fee: Amount,
    #[serde(default)]
    pub destination_tag: Option<u32>,
    #[serde(default)]
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    #[serde(default)]
    pub finish_after_blue_score: Option<u64>,
    #[serde(default)]
    pub cancel_after_blue_score: Option<u64>,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcEscrowCreateTx {
    pub fn validate_structure(&self) -> Result<(), DrcEscrowError> {
        if self.version != DRC_ESCROW_CREATE_TX_VERSION
            && self.version != DRC_ESCROW_CREATE_TICKET_VERSION
        {
            return Err(DrcEscrowError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_ESCROW_CREATE_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcEscrowError::TicketSelectorOnLegacyVersion);
        }
        if self.owner == Address::ZERO || self.recipient == Address::ZERO {
            return Err(DrcEscrowError::ZeroAddress);
        }
        if self.amount.as_base_units() == 0 {
            return Err(DrcEscrowError::ZeroAmount);
        }
        validate_escrow_time_bounds(self.finish_after_blue_score, self.cancel_after_blue_score)?;
        if self.invoice_id != Hash::ZERO {
            return Err(DrcEscrowError::NonZeroInvoiceNotSupported);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_ESCROW_CREATE_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("escrow create v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_ESCROW_CREATE_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_ESCROW_CREATE_TX_TYPE,
                self.version,
                self.owner,
                self.recipient,
                self.amount,
                self.fee,
                self.destination_tag,
                self.source_tag,
                self.invoice_id,
                self.finish_after_blue_score,
                self.cancel_after_blue_score,
                sequence,
            ))
            .expect("borsh escrow create v2");
        }
        borsh::to_vec(&(
            DRC_ESCROW_CREATE_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_ESCROW_CREATE_TX_TYPE,
            self.version,
            self.owner,
            self.recipient,
            self.amount,
            self.fee,
            self.destination_tag,
            self.source_tag,
            self.invoice_id,
            self.finish_after_blue_score,
            self.cancel_after_blue_score,
            self.nonce,
        ))
        .expect("borsh escrow create v1")
    }

    pub fn escrow_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    /// Optional destination tag authenticated on the wire (`Option<u32>` from v1).
    ///
    /// Unlike legacy payment v1/v2, escrow never encodes absence as bare `0`; `Some(0)` is a
    /// present tag and satisfies RequireDestTag when policy is active.
    pub fn authenticated_destination_tag(&self) -> Option<u32> {
        self.destination_tag
    }
}

impl BorshSerialize for DrcEscrowCreateTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.recipient, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.destination_tag, writer)?;
        BorshSerialize::serialize(&self.source_tag, writer)?;
        BorshSerialize::serialize(&self.invoice_id, writer)?;
        BorshSerialize::serialize(&self.finish_after_blue_score, writer)?;
        BorshSerialize::serialize(&self.cancel_after_blue_score, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcEscrowCreateTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            owner: Address::deserialize_reader(reader)?,
            recipient: Address::deserialize_reader(reader)?,
            amount: Amount::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            destination_tag: Option::<u32>::deserialize_reader(reader)?,
            source_tag: Option::<u32>::deserialize_reader(reader)?,
            invoice_id: Hash::deserialize_reader(reader)?,
            finish_after_blue_score: Option::<u64>::deserialize_reader(reader)?,
            cancel_after_blue_score: Option::<u64>::deserialize_reader(reader)?,
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
pub struct DrcEscrowFinishTx {
    pub version: u32,
    pub submitter: Address,
    pub escrow_id: Hash,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcEscrowFinishTx {
    pub fn validate_structure(&self) -> Result<(), DrcEscrowError> {
        if self.version != DRC_ESCROW_FINISH_TX_VERSION
            && self.version != DRC_ESCROW_FINISH_TICKET_VERSION
        {
            return Err(DrcEscrowError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_ESCROW_FINISH_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcEscrowError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.escrow_id == Hash::ZERO {
            return Err(DrcEscrowError::ZeroAddress);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_ESCROW_FINISH_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("escrow finish v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_ESCROW_FINISH_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_ESCROW_FINISH_TX_TYPE,
                self.version,
                self.submitter,
                self.escrow_id,
                self.fee,
                sequence,
            ))
            .expect("borsh escrow finish v2");
        }
        borsh::to_vec(&(
            DRC_ESCROW_FINISH_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_ESCROW_FINISH_TX_TYPE,
            self.version,
            self.submitter,
            self.escrow_id,
            self.fee,
            self.nonce,
        ))
        .expect("borsh escrow finish v1")
    }

    pub fn finish_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcEscrowFinishTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.escrow_id, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcEscrowFinishTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            escrow_id: Hash::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
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
pub struct DrcEscrowCancelTx {
    pub version: u32,
    /// Any funded DRC account may submit cancel (rippled EscrowCancel); value returns to escrow owner.
    pub submitter: Address,
    pub escrow_id: Hash,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcEscrowCancelTx {
    pub fn validate_structure(&self) -> Result<(), DrcEscrowError> {
        if self.version != DRC_ESCROW_CANCEL_TX_VERSION
            && self.version != DRC_ESCROW_CANCEL_TICKET_VERSION
        {
            return Err(DrcEscrowError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_ESCROW_CANCEL_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcEscrowError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.escrow_id == Hash::ZERO {
            return Err(DrcEscrowError::ZeroAddress);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_ESCROW_CANCEL_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("escrow cancel v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_ESCROW_CANCEL_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_ESCROW_CANCEL_TX_TYPE,
                self.version,
                self.submitter,
                self.escrow_id,
                self.fee,
                sequence,
            ))
            .expect("borsh escrow cancel v2");
        }
        borsh::to_vec(&(
            DRC_ESCROW_CANCEL_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_ESCROW_CANCEL_TX_TYPE,
            self.version,
            self.submitter,
            self.escrow_id,
            self.fee,
            self.nonce,
        ))
        .expect("borsh escrow cancel v1")
    }

    pub fn cancel_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcEscrowCancelTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.escrow_id, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcEscrowCancelTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            escrow_id: Hash::deserialize_reader(reader)?,
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
pub struct DrcEscrowLive {
    pub version: u32,
    pub escrow_id: Hash,
    pub owner: Address,
    pub recipient: Address,
    pub amount: Amount,
    pub destination_tag: Option<u32>,
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    pub finish_after_blue_score: Option<u64>,
    pub cancel_after_blue_score: Option<u64>,
    pub create_blue_score: u64,
}

impl DrcEscrowLive {
    pub fn validate(&self) -> Result<(), DrcEscrowError> {
        if self.version != DRC_ESCROW_LIVE_STATE_VERSION {
            return Err(DrcEscrowError::UnsupportedStateVersion(self.version));
        }
        if self.escrow_id == Hash::ZERO {
            return Err(DrcEscrowError::ZeroAddress);
        }
        validate_escrow_time_bounds(self.finish_after_blue_score, self.cancel_after_blue_score)?;
        Ok(())
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcEscrowReceipt {
    pub version: u32,
    pub escrow_id: Hash,
    pub outcome: DrcEscrowOutcome,
    pub owner: Address,
    pub recipient: Address,
    pub amount: Amount,
    pub destination_tag: Option<u32>,
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    pub settlement_blue_score: u64,
    pub settlement_tx_id: Hash,
}

impl DrcEscrowReceipt {
    pub fn validate(&self) -> Result<(), DrcEscrowError> {
        if self.version != DRC_ESCROW_RECEIPT_VERSION {
            return Err(DrcEscrowError::UnsupportedReceiptVersion(self.version));
        }
        Ok(())
    }
}

pub fn validate_escrow_time_bounds(
    finish_after: Option<u64>,
    cancel_after: Option<u64>,
) -> Result<(), DrcEscrowError> {
    if finish_after.is_none() && cancel_after.is_none() {
        return Err(DrcEscrowError::MissingTimeBound);
    }
    for bound in [finish_after, cancel_after].into_iter().flatten() {
        if bound > DRC_ESCROW_MAX_BLUE_SCORE_BOUND {
            return Err(DrcEscrowError::BlueScoreBoundOverflow);
        }
    }
    if let (Some(f), Some(c)) = (finish_after, cancel_after) {
        if f >= c {
            return Err(DrcEscrowError::FinishNotBeforeCancel);
        }
    }
    Ok(())
}

pub fn escrow_finish_allowed(
    score: u64,
    finish_after: Option<u64>,
    cancel_after: Option<u64>,
) -> bool {
    if let Some(f) = finish_after {
        if score < f {
            return false;
        }
    }
    if let Some(c) = cancel_after {
        if score >= c {
            return false;
        }
    }
    true
}

pub fn escrow_cancel_allowed(score: u64, cancel_after: Option<u64>) -> bool {
    cancel_after.is_some_and(|c| score >= c)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcEscrowError {
    #[error("unsupported DRC escrow version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC escrow state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("unsupported DRC escrow receipt version {0}")]
    UnsupportedReceiptVersion(u32),
    #[error("ticket selector on legacy escrow version")]
    TicketSelectorOnLegacyVersion,
    #[error("zero address or escrow id")]
    ZeroAddress,
    #[error("zero escrow amount")]
    ZeroAmount,
    #[error("escrow requires at least one blue-score time bound")]
    MissingTimeBound,
    #[error("escrow blue-score bound overflow")]
    BlueScoreBoundOverflow,
    #[error("finish_after must be strictly less than cancel_after")]
    FinishNotBeforeCancel,
    #[error("escrow v1 does not support merchant invoice_id (must be zero)")]
    NonZeroInvoiceNotSupported,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Address;

    #[test]
    fn authenticated_destination_tag_some_zero_is_present() {
        let tx = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: Address([1; 20]),
            recipient: Address([2; 20]),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            destination_tag: Some(0),
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(10),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert_eq!(tx.authenticated_destination_tag(), Some(0));
        assert!(tx.validate_structure().is_ok());
    }

    #[test]
    fn nonzero_invoice_id_rejected_at_structure() {
        let tx = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: Address([1; 20]),
            recipient: Address([2; 20]),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash([3; 32]),
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(10),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(matches!(
            tx.validate_structure(),
            Err(DrcEscrowError::NonZeroInvoiceNotSupported)
        ));
    }
}
