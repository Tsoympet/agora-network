//! Explicit DRC account sequence selector (ordinary nonce vs one-use ticket).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Maximum live outstanding tickets per DRC account (hard protocol cap).
///
/// Rippled allows large ticket counts tied to owner reserve; Agora has no account
/// reserve in this slice, so growth is bounded by this constant instead.
pub const DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT: usize = 32;

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    PartialOrd,
    Ord,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcAccountSequence {
    /// Shared ordinary account nonce (strictly sequential when used).
    Nonce = 0,
    /// One-use ticket sequence reserved at creation (may execute out of order).
    Ticket = 1,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcAccountSequenceSelector {
    pub kind: DrcAccountSequence,
    pub value: u64,
}

impl DrcAccountSequenceSelector {
    pub const fn nonce(value: u64) -> Self {
        Self {
            kind: DrcAccountSequence::Nonce,
            value,
        }
    }

    pub const fn ticket(sequence: u64) -> Self {
        Self {
            kind: DrcAccountSequence::Ticket,
            value: sequence,
        }
    }

    pub fn validate(&self) -> Result<(), DrcAccountSequenceError> {
        match self.kind {
            DrcAccountSequence::Nonce | DrcAccountSequence::Ticket => Ok(()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcAccountSequenceError {
    #[error("invalid DRC account sequence selector")]
    InvalidSelector,
    #[error("DRC ticket sequence overflow")]
    TicketSequenceOverflow,
    #[error("DRC account nonce overflow")]
    NonceOverflow,
    #[error("DRC outstanding ticket cap exceeded")]
    TicketCapExceeded,
    #[error("unknown DRC ticket sequence")]
    UnknownTicket,
    #[error("duplicate DRC ticket sequence")]
    DuplicateTicket,
    #[error("ticket sequence selector requires a ticket-capable operation version")]
    TicketSelectorOnLegacyVersion,
    #[error("ticket-capable operation version requires account_sequence")]
    MissingAccountSequence,
}

/// Resolve the bound sequence for a DRC account operation envelope.
pub fn resolve_drc_account_sequence(
    version: u32,
    ticket_capable_version: u32,
    nonce: u64,
    account_sequence: Option<DrcAccountSequenceSelector>,
) -> Result<DrcAccountSequenceSelector, DrcAccountSequenceError> {
    if version >= ticket_capable_version {
        let selector = account_sequence.ok_or(DrcAccountSequenceError::MissingAccountSequence)?;
        selector.validate()?;
        Ok(selector)
    } else {
        if account_sequence.is_some_and(|s| s.kind == DrcAccountSequence::Ticket) {
            return Err(DrcAccountSequenceError::TicketSelectorOnLegacyVersion);
        }
        Ok(DrcAccountSequenceSelector::nonce(nonce))
    }
}
