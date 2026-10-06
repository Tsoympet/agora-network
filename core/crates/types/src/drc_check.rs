//! Typed, contract-free native DRC checks (exact-value authorization; no create-time lock).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash};

pub const DRC_MAX_LIVE_CHECKS_PER_ACCOUNT: usize = 32;

pub const DRC_CHECK_CREATE_TX_VERSION: u32 = 1;
pub const DRC_CHECK_CREATE_TICKET_VERSION: u32 = 2;
pub const DRC_CHECK_CASH_TX_VERSION: u32 = 1;
pub const DRC_CHECK_CASH_TICKET_VERSION: u32 = 2;
pub const DRC_CHECK_CANCEL_TX_VERSION: u32 = 1;
pub const DRC_CHECK_CANCEL_TICKET_VERSION: u32 = 2;
pub const DRC_CHECK_LIVE_STATE_VERSION: u32 = 1;
pub const DRC_CHECK_RECEIPT_VERSION: u32 = 1;

pub const DRC_CHECK_CREATE_TX_TYPE: &[u8] = b"drc_check_create";
pub const DRC_CHECK_CASH_TX_TYPE: &[u8] = b"drc_check_cash";
pub const DRC_CHECK_CANCEL_TX_TYPE: &[u8] = b"drc_check_cancel";

pub const DRC_CHECK_CREATE_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-check-create-v1";
pub const DRC_CHECK_CREATE_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-check-create-v2";
pub const DRC_CHECK_CASH_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-check-cash-v1";
pub const DRC_CHECK_CASH_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-check-cash-v2";
pub const DRC_CHECK_CANCEL_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-check-cancel-v1";
pub const DRC_CHECK_CANCEL_TICKET_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-check-cancel-v2";

pub const DRC_CHECK_MAX_BLUE_SCORE_BOUND: u64 = u64::MAX - 1;

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcCheckOutcome {
    Cashed = 1,
    Cancelled = 2,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcCheckCreateTx {
    pub version: u32,
    pub owner: Address,
    pub destination: Address,
    pub amount: Amount,
    pub fee: Amount,
    #[serde(default)]
    pub destination_tag: Option<u32>,
    #[serde(default)]
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
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

impl DrcCheckCreateTx {
    pub fn validate_structure(&self) -> Result<(), DrcCheckError> {
        if self.version != DRC_CHECK_CREATE_TX_VERSION
            && self.version != DRC_CHECK_CREATE_TICKET_VERSION
        {
            return Err(DrcCheckError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_CHECK_CREATE_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcCheckError::TicketSelectorOnLegacyVersion);
        }
        if self.owner == Address::ZERO || self.destination == Address::ZERO {
            return Err(DrcCheckError::ZeroAddress);
        }
        if self.amount.as_base_units() == 0 {
            return Err(DrcCheckError::ZeroAmount);
        }
        if self.invoice_id != Hash::ZERO {
            return Err(DrcCheckError::NonZeroInvoiceNotSupported);
        }
        validate_check_expiration_bound(self.expires_after_blue_score)?;
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_CHECK_CREATE_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("check create v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_CHECK_CREATE_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_CHECK_CREATE_TX_TYPE,
                self.version,
                self.owner,
                self.destination,
                self.amount,
                self.fee,
                self.destination_tag,
                self.source_tag,
                self.invoice_id,
                self.expires_after_blue_score,
                sequence,
            ))
            .expect("borsh check create v2");
        }
        borsh::to_vec(&(
            DRC_CHECK_CREATE_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_CHECK_CREATE_TX_TYPE,
            self.version,
            self.owner,
            self.destination,
            self.amount,
            self.fee,
            self.destination_tag,
            self.source_tag,
            self.invoice_id,
            self.expires_after_blue_score,
            self.nonce,
        ))
        .expect("borsh check create v1")
    }

    pub fn check_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn authenticated_destination_tag(&self) -> Option<u32> {
        self.destination_tag
    }
}

impl BorshSerialize for DrcCheckCreateTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.destination, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.destination_tag, writer)?;
        BorshSerialize::serialize(&self.source_tag, writer)?;
        BorshSerialize::serialize(&self.invoice_id, writer)?;
        BorshSerialize::serialize(&self.expires_after_blue_score, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcCheckCreateTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            owner: Address::deserialize_reader(reader)?,
            destination: Address::deserialize_reader(reader)?,
            amount: Amount::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            destination_tag: Option::<u32>::deserialize_reader(reader)?,
            source_tag: Option::<u32>::deserialize_reader(reader)?,
            invoice_id: Hash::deserialize_reader(reader)?,
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
pub struct DrcCheckCashTx {
    pub version: u32,
    pub submitter: Address,
    pub check_id: Hash,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcCheckCashTx {
    pub fn validate_structure(&self) -> Result<(), DrcCheckError> {
        if self.version != DRC_CHECK_CASH_TX_VERSION
            && self.version != DRC_CHECK_CASH_TICKET_VERSION
        {
            return Err(DrcCheckError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_CHECK_CASH_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcCheckError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.check_id == Hash::ZERO {
            return Err(DrcCheckError::ZeroAddress);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_CHECK_CASH_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("check cash v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_CHECK_CASH_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_CHECK_CASH_TX_TYPE,
                self.version,
                self.submitter,
                self.check_id,
                self.fee,
                sequence,
            ))
            .expect("borsh check cash v2");
        }
        borsh::to_vec(&(
            DRC_CHECK_CASH_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_CHECK_CASH_TX_TYPE,
            self.version,
            self.submitter,
            self.check_id,
            self.fee,
            self.nonce,
        ))
        .expect("borsh check cash v1")
    }

    pub fn cash_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcCheckCashTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.check_id, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcCheckCashTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            check_id: Hash::deserialize_reader(reader)?,
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
pub struct DrcCheckCancelTx {
    pub version: u32,
    pub submitter: Address,
    pub check_id: Hash,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcCheckCancelTx {
    pub fn validate_structure(&self) -> Result<(), DrcCheckError> {
        if self.version != DRC_CHECK_CANCEL_TX_VERSION
            && self.version != DRC_CHECK_CANCEL_TICKET_VERSION
        {
            return Err(DrcCheckError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_CHECK_CANCEL_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcCheckError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.check_id == Hash::ZERO {
            return Err(DrcCheckError::ZeroAddress);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_CHECK_CANCEL_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("check cancel v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_CHECK_CANCEL_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_CHECK_CANCEL_TX_TYPE,
                self.version,
                self.submitter,
                self.check_id,
                self.fee,
                sequence,
            ))
            .expect("borsh check cancel v2");
        }
        borsh::to_vec(&(
            DRC_CHECK_CANCEL_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_CHECK_CANCEL_TX_TYPE,
            self.version,
            self.submitter,
            self.check_id,
            self.fee,
            self.nonce,
        ))
        .expect("borsh check cancel v1")
    }

    pub fn cancel_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcCheckCancelTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.check_id, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcCheckCancelTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            check_id: Hash::deserialize_reader(reader)?,
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
pub struct DrcCheckLive {
    pub version: u32,
    pub check_id: Hash,
    pub owner: Address,
    pub destination: Address,
    pub amount: Amount,
    pub destination_tag: Option<u32>,
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    pub expires_after_blue_score: Option<u64>,
    pub create_blue_score: u64,
}

impl DrcCheckLive {
    pub fn validate(&self) -> Result<(), DrcCheckError> {
        if self.version != DRC_CHECK_LIVE_STATE_VERSION {
            return Err(DrcCheckError::UnsupportedStateVersion(self.version));
        }
        if self.check_id == Hash::ZERO {
            return Err(DrcCheckError::ZeroAddress);
        }
        validate_check_expiration_bound(self.expires_after_blue_score)?;
        Ok(())
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcCheckReceipt {
    pub version: u32,
    pub check_id: Hash,
    pub outcome: DrcCheckOutcome,
    pub owner: Address,
    pub destination: Address,
    pub amount: Amount,
    pub destination_tag: Option<u32>,
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    pub settlement_blue_score: u64,
    pub settlement_tx_id: Hash,
}

impl DrcCheckReceipt {
    pub fn validate(&self) -> Result<(), DrcCheckError> {
        if self.version != DRC_CHECK_RECEIPT_VERSION {
            return Err(DrcCheckError::UnsupportedReceiptVersion(self.version));
        }
        Ok(())
    }
}

pub fn validate_check_expiration_bound(expires_after: Option<u64>) -> Result<(), DrcCheckError> {
    if let Some(b) = expires_after {
        if b > DRC_CHECK_MAX_BLUE_SCORE_BOUND {
            return Err(DrcCheckError::BlueScoreBoundOverflow);
        }
    }
    Ok(())
}

/// Cash allowed while `score < expires`; rejected at `score >= expires`.
pub fn check_cash_allowed(score: u64, expires_after: Option<u64>) -> bool {
    !expires_after.is_some_and(|e| score >= e)
}

/// Before expiration: owner or destination. At/after expiration: any submitter (validated in apply).
pub fn check_cancel_submitter_allowed(
    score: u64,
    expires_after: Option<u64>,
    submitter: Address,
    owner: Address,
    destination: Address,
) -> bool {
    if expires_after.is_some_and(|e| score >= e) {
        return true;
    }
    submitter == owner || submitter == destination
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DrcCheckError {
    #[error("unsupported version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported live state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("unsupported receipt version {0}")]
    UnsupportedReceiptVersion(u32),
    #[error("zero address or id")]
    ZeroAddress,
    #[error("zero amount")]
    ZeroAmount,
    #[error("non-zero invoice_id not supported")]
    NonZeroInvoiceNotSupported,
    #[error("ticket selector on legacy tx version")]
    TicketSelectorOnLegacyVersion,
    #[error("blue score bound overflow")]
    BlueScoreBoundOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_id_stable_for_create_body() {
        let tx = DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TX_VERSION,
            owner: Address([1; 20]),
            destination: Address([2; 20]),
            amount: Amount::from_base_units(5),
            fee: Amount::from_base_units(1),
            destination_tag: Some(0),
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        };
        tx.validate_structure().unwrap();
        assert_ne!(tx.check_id(), Hash::ZERO);
    }

    #[test]
    fn expiration_boundary_inclusive_at_score() {
        assert!(check_cash_allowed(99, Some(100)));
        assert!(!check_cash_allowed(100, Some(100)));
    }
}
