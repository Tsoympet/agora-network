//! Typed, contract-free native DRC payment channels (locked DRC + secp256k1 cumulative claims).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash};

pub const DRC_MAX_LIVE_PAYMENT_CHANNELS_PER_ACCOUNT: usize = 32;

pub const DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION: u32 = 2;
pub const DRC_PAYMENT_CHANNEL_FUND_TX_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION: u32 = 2;
pub const DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION: u32 = 2;
pub const DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION: u32 = 2;
pub const DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_RECEIPT_VERSION: u32 = 2;
pub const DRC_PAYMENT_CHANNEL_FUND_EVENT_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_CLAIM_EVENT_VERSION: u32 = 1;
pub const DRC_PAYMENT_CHANNEL_SCHEDULE_EVENT_VERSION: u32 = 1;

pub const DRC_PAYMENT_CHANNEL_CREATE_TX_TYPE: &[u8] = b"drc_payment_channel_create";
pub const DRC_PAYMENT_CHANNEL_FUND_TX_TYPE: &[u8] = b"drc_payment_channel_fund";
pub const DRC_PAYMENT_CHANNEL_CLAIM_TX_TYPE: &[u8] = b"drc_payment_channel_claim";
pub const DRC_PAYMENT_CHANNEL_CLOSE_TX_TYPE: &[u8] = b"drc_payment_channel_close";

pub const DRC_PAYMENT_CHANNEL_CREATE_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-create-v1";
pub const DRC_PAYMENT_CHANNEL_CREATE_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-create-v2";
pub const DRC_PAYMENT_CHANNEL_FUND_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-fund-v1";
pub const DRC_PAYMENT_CHANNEL_FUND_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-fund-v2";
pub const DRC_PAYMENT_CHANNEL_CLAIM_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-claim-v1";
pub const DRC_PAYMENT_CHANNEL_CLAIM_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-claim-v2";
pub const DRC_PAYMENT_CHANNEL_CLOSE_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-close-v1";
pub const DRC_PAYMENT_CHANNEL_CLOSE_TICKET_SIGNING_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-close-v2";

pub const DRC_PAYMENT_CHANNEL_OFFLEDGER_CLAIM_DOMAIN: &[u8] =
    b"agora-trident-drc-payment-channel-offledger-claim-v1";

pub const DRC_PAYMENT_CHANNEL_MAX_BLUE_SCORE_BOUND: u64 = u64::MAX - 1;

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcPaymentChannelCloseKind {
    OwnerScheduleClose = 1,
    DestinationClose = 2,
    Finalize = 3,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcPaymentChannelOutcome {
    Closed = 1,
}

pub fn validate_payment_channel_blue_score_bound(
    bound: Option<u64>,
) -> Result<(), DrcPaymentChannelError> {
    if let Some(score) = bound {
        if score > DRC_PAYMENT_CHANNEL_MAX_BLUE_SCORE_BOUND {
            return Err(DrcPaymentChannelError::BlueScoreBoundOverflow);
        }
    }
    Ok(())
}

pub fn payment_channel_owner_schedule_deadline(
    application_blue_score: u64,
    settle_delay_blue_scores: u64,
) -> Result<u64, DrcPaymentChannelError> {
    application_blue_score
        .checked_add(settle_delay_blue_scores)
        .ok_or(DrcPaymentChannelError::BlueScoreBoundOverflow)
}

/// `CancelAfter` must be strictly after the containing block blue score at create.
pub fn payment_channel_cancel_after_valid_at_create(
    application_blue_score: u64,
    cancel_after_blue_score: Option<u64>,
) -> Result<(), DrcPaymentChannelError> {
    if let Some(cancel) = cancel_after_blue_score {
        if cancel <= application_blue_score {
            return Err(DrcPaymentChannelError::CancelAfterNotFuture);
        }
        validate_payment_channel_blue_score_bound(Some(cancel))?;
    }
    Ok(())
}

/// Inclusive finalize boundary: `blue_score >= deadline`.
pub fn payment_channel_finalize_allowed(
    blue_score: u64,
    close_finalizable_after: Option<u64>,
    cancel_after_blue_score: Option<u64>,
) -> bool {
    if let Some(after) = cancel_after_blue_score {
        if blue_score >= after {
            return true;
        }
    }
    if let Some(finalize) = close_finalizable_after {
        if blue_score >= finalize {
            return true;
        }
    }
    false
}

pub fn payment_channel_offledger_claim_signing_bytes(
    chain_id: &str,
    genesis: &Hash,
    channel_id: &Hash,
    cumulative_authorized: Amount,
) -> Vec<u8> {
    borsh::to_vec(&(
        DRC_PAYMENT_CHANNEL_OFFLEDGER_CLAIM_DOMAIN,
        chain_id,
        genesis.as_bytes(),
        channel_id,
        cumulative_authorized,
    ))
    .expect("borsh payment channel off-ledger claim")
}

pub fn payment_channel_fund_submitter_allowed(submitter: Address, owner: Address) -> bool {
    submitter == owner
}

pub fn payment_channel_claim_submitter_allowed(submitter: Address, destination: Address) -> bool {
    submitter == destination
}

/// rippled: source may `tfClose` to start settle delay; destination may close immediately.
/// Agora bounded subset: owner schedules; destination closes immediately; owner or destination
/// may finalize once `payment_channel_finalize_allowed` is true (fund is owner-only).
pub fn payment_channel_close_submitter_allowed(
    close_kind: DrcPaymentChannelCloseKind,
    submitter: Address,
    owner: Address,
    destination: Address,
) -> bool {
    match close_kind {
        DrcPaymentChannelCloseKind::OwnerScheduleClose => submitter == owner,
        DrcPaymentChannelCloseKind::DestinationClose => submitter == destination,
        DrcPaymentChannelCloseKind::Finalize => submitter == owner || submitter == destination,
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DrcPaymentChannelCreateTx {
    pub version: u32,
    pub owner: Address,
    pub destination: Address,
    pub amount: Amount,
    pub fee: Amount,
    pub claim_public_key: Vec<u8>,
    pub settle_delay_blue_scores: u64,
    #[serde(default)]
    pub destination_tag: Option<u32>,
    #[serde(default)]
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
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

impl DrcPaymentChannelCreateTx {
    pub fn validate_structure(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION
            && self.version != DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION
        {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcPaymentChannelError::TicketSelectorOnLegacyVersion);
        }
        if self.owner == Address::ZERO
            || self.destination == Address::ZERO
            || self.owner == self.destination
        {
            return Err(DrcPaymentChannelError::ZeroAddress);
        }
        if self.amount.as_base_units() == 0 {
            return Err(DrcPaymentChannelError::ZeroAmount);
        }
        if self.fee.as_base_units() == 0 {
            return Err(DrcPaymentChannelError::ZeroFee);
        }
        if self.claim_public_key.len() != 33 {
            return Err(DrcPaymentChannelError::MalformedClaimPublicKey);
        }
        if self.settle_delay_blue_scores == 0 {
            return Err(DrcPaymentChannelError::ZeroSettleDelay);
        }
        validate_payment_channel_blue_score_bound(self.cancel_after_blue_score)?;
        if self.invoice_id != Hash::ZERO {
            return Err(DrcPaymentChannelError::NonZeroInvoiceNotSupported);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("payment channel create v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_PAYMENT_CHANNEL_CREATE_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_PAYMENT_CHANNEL_CREATE_TX_TYPE,
                self.version,
                self.owner,
                self.destination,
                self.amount,
                self.fee,
                &self.claim_public_key,
                self.settle_delay_blue_scores,
                self.destination_tag,
                self.source_tag,
                self.invoice_id,
                self.cancel_after_blue_score,
                sequence,
            ))
            .expect("borsh payment channel create v2");
        }
        borsh::to_vec(&(
            DRC_PAYMENT_CHANNEL_CREATE_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_PAYMENT_CHANNEL_CREATE_TX_TYPE,
            self.version,
            self.owner,
            self.destination,
            self.amount,
            self.fee,
            &self.claim_public_key,
            self.settle_delay_blue_scores,
            self.destination_tag,
            self.source_tag,
            self.invoice_id,
            self.cancel_after_blue_score,
            self.nonce,
        ))
        .expect("borsh payment channel create v1")
    }

    pub fn channel_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn authenticated_destination_tag(&self) -> Option<u32> {
        self.destination_tag
    }
}

impl BorshSerialize for DrcPaymentChannelCreateTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.destination, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.claim_public_key, writer)?;
        BorshSerialize::serialize(&self.settle_delay_blue_scores, writer)?;
        BorshSerialize::serialize(&self.destination_tag, writer)?;
        BorshSerialize::serialize(&self.source_tag, writer)?;
        BorshSerialize::serialize(&self.invoice_id, writer)?;
        BorshSerialize::serialize(&self.cancel_after_blue_score, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcPaymentChannelCreateTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            owner: Address::deserialize_reader(reader)?,
            destination: Address::deserialize_reader(reader)?,
            amount: Amount::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            claim_public_key: Vec::<u8>::deserialize_reader(reader)?,
            settle_delay_blue_scores: u64::deserialize_reader(reader)?,
            destination_tag: Option::<u32>::deserialize_reader(reader)?,
            source_tag: Option::<u32>::deserialize_reader(reader)?,
            invoice_id: Hash::deserialize_reader(reader)?,
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
pub struct DrcPaymentChannelFundTx {
    pub version: u32,
    pub submitter: Address,
    pub channel_id: Hash,
    pub amount: Amount,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcPaymentChannelFundTx {
    pub fn validate_structure(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_FUND_TX_VERSION
            && self.version != DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION
        {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcPaymentChannelError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.channel_id == Hash::ZERO {
            return Err(DrcPaymentChannelError::ZeroAddress);
        }
        if self.amount.as_base_units() == 0 {
            return Err(DrcPaymentChannelError::ZeroAmount);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("payment channel fund v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_PAYMENT_CHANNEL_FUND_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_PAYMENT_CHANNEL_FUND_TX_TYPE,
                self.version,
                self.submitter,
                self.channel_id,
                self.amount,
                self.fee,
                sequence,
            ))
            .expect("borsh payment channel fund v2");
        }
        borsh::to_vec(&(
            DRC_PAYMENT_CHANNEL_FUND_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_PAYMENT_CHANNEL_FUND_TX_TYPE,
            self.version,
            self.submitter,
            self.channel_id,
            self.amount,
            self.fee,
            self.nonce,
        ))
        .expect("borsh payment channel fund v1")
    }

    pub fn fund_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcPaymentChannelFundTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.channel_id, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcPaymentChannelFundTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            channel_id: Hash::deserialize_reader(reader)?,
            amount: Amount::deserialize_reader(reader)?,
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
pub struct DrcPaymentChannelClaimTx {
    pub version: u32,
    pub submitter: Address,
    pub channel_id: Hash,
    pub cumulative_authorized: Amount,
    pub channel_claim_signature: Vec<u8>,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcPaymentChannelClaimTx {
    pub fn validate_structure(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION
            && self.version != DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION
        {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcPaymentChannelError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.channel_id == Hash::ZERO {
            return Err(DrcPaymentChannelError::ZeroAddress);
        }
        if self.cumulative_authorized.as_base_units() == 0 {
            return Err(DrcPaymentChannelError::ZeroAmount);
        }
        if self.channel_claim_signature.len() != 64 {
            return Err(DrcPaymentChannelError::MalformedClaimSignature);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("payment channel claim v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_PAYMENT_CHANNEL_CLAIM_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_PAYMENT_CHANNEL_CLAIM_TX_TYPE,
                self.version,
                self.submitter,
                self.channel_id,
                self.cumulative_authorized,
                &self.channel_claim_signature,
                self.fee,
                sequence,
            ))
            .expect("borsh payment channel claim v2");
        }
        borsh::to_vec(&(
            DRC_PAYMENT_CHANNEL_CLAIM_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_PAYMENT_CHANNEL_CLAIM_TX_TYPE,
            self.version,
            self.submitter,
            self.channel_id,
            self.cumulative_authorized,
            &self.channel_claim_signature,
            self.fee,
            self.nonce,
        ))
        .expect("borsh payment channel claim v1")
    }

    pub fn claim_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcPaymentChannelClaimTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.channel_id, writer)?;
        BorshSerialize::serialize(&self.cumulative_authorized, writer)?;
        BorshSerialize::serialize(&self.channel_claim_signature, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcPaymentChannelClaimTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            channel_id: Hash::deserialize_reader(reader)?,
            cumulative_authorized: Amount::deserialize_reader(reader)?,
            channel_claim_signature: Vec::<u8>::deserialize_reader(reader)?,
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
pub struct DrcPaymentChannelCloseTx {
    pub version: u32,
    pub submitter: Address,
    pub channel_id: Hash,
    pub close_kind: DrcPaymentChannelCloseKind,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcPaymentChannelCloseTx {
    pub fn validate_structure(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION
            && self.version != DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION
        {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        if self.version < DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcPaymentChannelError::TicketSelectorOnLegacyVersion);
        }
        if self.submitter == Address::ZERO || self.channel_id == Hash::ZERO {
            return Err(DrcPaymentChannelError::ZeroAddress);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("payment channel close v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_PAYMENT_CHANNEL_CLOSE_TICKET_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_PAYMENT_CHANNEL_CLOSE_TX_TYPE,
                self.version,
                self.submitter,
                self.channel_id,
                self.close_kind,
                self.fee,
                sequence,
            ))
            .expect("borsh payment channel close v2");
        }
        borsh::to_vec(&(
            DRC_PAYMENT_CHANNEL_CLOSE_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_PAYMENT_CHANNEL_CLOSE_TX_TYPE,
            self.version,
            self.submitter,
            self.channel_id,
            self.close_kind,
            self.fee,
            self.nonce,
        ))
        .expect("borsh payment channel close v1")
    }

    pub fn close_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

impl BorshSerialize for DrcPaymentChannelCloseTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.submitter, writer)?;
        BorshSerialize::serialize(&self.channel_id, writer)?;
        BorshSerialize::serialize(&self.close_kind, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.account_sequence, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcPaymentChannelCloseTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            submitter: Address::deserialize_reader(reader)?,
            channel_id: Hash::deserialize_reader(reader)?,
            close_kind: DrcPaymentChannelCloseKind::deserialize_reader(reader)?,
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
pub struct DrcPaymentChannelLive {
    pub version: u32,
    pub channel_id: Hash,
    pub owner: Address,
    pub destination: Address,
    pub destination_tag: Option<u32>,
    pub source_tag: Option<u32>,
    pub invoice_id: Hash,
    pub claim_public_key: Vec<u8>,
    pub settle_delay_blue_scores: u64,
    pub cancel_after_blue_score: Option<u64>,
    pub total_funded: Amount,
    pub cumulative_claimed: Amount,
    pub close_finalizable_after: Option<u64>,
    pub create_blue_score: u64,
}

impl DrcPaymentChannelLive {
    pub fn validate(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        if self.claim_public_key.len() != 33 {
            return Err(DrcPaymentChannelError::MalformedClaimPublicKey);
        }
        if self.cumulative_claimed.as_base_units() > self.total_funded.as_base_units() {
            return Err(DrcPaymentChannelError::ClaimExceedsFunded);
        }
        Ok(())
    }

    pub fn locked_remainder(&self) -> Result<Amount, DrcPaymentChannelError> {
        let rem = self
            .total_funded
            .as_base_units()
            .checked_sub(self.cumulative_claimed.as_base_units())
            .ok_or(DrcPaymentChannelError::ClaimExceedsFunded)?;
        Ok(Amount::from_base_units(rem))
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcPaymentChannelReceipt {
    pub version: u32,
    pub channel_id: Hash,
    pub outcome: DrcPaymentChannelOutcome,
    pub owner: Address,
    pub destination: Address,
    pub destination_tag: Option<u32>,
    pub source_tag: Option<u32>,
    pub total_funded: Amount,
    pub cumulative_claimed: Amount,
    pub settlement_blue_score: u64,
    pub settlement_tx_id: Hash,
}

impl DrcPaymentChannelReceipt {
    pub fn validate(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_RECEIPT_VERSION {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        Ok(())
    }
}

/// Immutable fund audit record (keyed by `fund_tx_id`).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcPaymentChannelFundEvent {
    pub version: u32,
    pub channel_id: Hash,
    pub fund_tx_id: Hash,
    pub amount: Amount,
    pub total_funded_after: Amount,
    pub application_blue_score: u64,
}

impl DrcPaymentChannelFundEvent {
    pub fn validate(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_FUND_EVENT_VERSION {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        Ok(())
    }
}

/// Immutable claim audit record (keyed by `claim_tx_id`).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcPaymentChannelClaimEvent {
    pub version: u32,
    pub channel_id: Hash,
    pub claim_tx_id: Hash,
    pub cumulative_authorized: Amount,
    pub claim_delta: Amount,
    pub application_blue_score: u64,
}

impl DrcPaymentChannelClaimEvent {
    pub fn validate(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_CLAIM_EVENT_VERSION {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        Ok(())
    }
}

/// Immutable owner schedule-close audit record (keyed by `close_tx_id`).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcPaymentChannelScheduleEvent {
    pub version: u32,
    pub channel_id: Hash,
    pub close_tx_id: Hash,
    pub close_finalizable_after: u64,
    pub application_blue_score: u64,
}

impl DrcPaymentChannelScheduleEvent {
    pub fn validate(&self) -> Result<(), DrcPaymentChannelError> {
        if self.version != DRC_PAYMENT_CHANNEL_SCHEDULE_EVENT_VERSION {
            return Err(DrcPaymentChannelError::UnsupportedVersion(self.version));
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DrcPaymentChannelError {
    #[error("zero fee")]
    ZeroFee,
    #[error("cancel after must be strictly future at create")]
    CancelAfterNotFuture,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u32),
    #[error("zero address")]
    ZeroAddress,
    #[error("zero amount")]
    ZeroAmount,
    #[error("zero settle delay")]
    ZeroSettleDelay,
    #[error("malformed claim public key")]
    MalformedClaimPublicKey,
    #[error("non-zero invoice not supported")]
    NonZeroInvoiceNotSupported,
    #[error("ticket selector on legacy version")]
    TicketSelectorOnLegacyVersion,
    #[error("blue score bound overflow")]
    BlueScoreBoundOverflow,
    #[error("claim exceeds funded")]
    ClaimExceedsFunded,
    #[error("malformed claim signature")]
    MalformedClaimSignature,
}
