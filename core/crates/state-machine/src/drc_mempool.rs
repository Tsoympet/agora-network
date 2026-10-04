//! Mempool-facing DRC ticket sequence rules (policy only — not consensus authority).

use agora_types::{
    resolve_drc_account_sequence, Address, DrcAccountSequence, DrcAccountSequenceSelector,
    NativeAssetId,
};

use crate::drc_ticket::load_drc_account_tickets;
use crate::{StateError, StateStore};

/// How a ticket-capable DRC operation reserves mempool slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrcMempoolReservation {
    /// Ordinary shared account nonce (one pending op per owner).
    AccountNonce,
    /// Single-use ticket sequence (one pending consumer per `(owner, sequence)`).
    Ticket { sequence: u64 },
}

/// Resolve selector + enforce public admission: ticket spends require canonical tickets.
pub fn plan_drc_mempool_reservation(
    store: &StateStore,
    owner: &Address,
    version: u32,
    ticket_capable_version: u32,
    nonce: u64,
    account_sequence: Option<DrcAccountSequenceSelector>,
) -> Result<DrcMempoolReservation, StateError> {
    let selector =
        resolve_drc_account_sequence(version, ticket_capable_version, nonce, account_sequence)
            .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    match selector.kind {
        DrcAccountSequence::Nonce => Ok(DrcMempoolReservation::AccountNonce),
        DrcAccountSequence::Ticket => {
            let tickets = load_drc_account_tickets(store, owner)?;
            if !tickets.contains(&selector.value) {
                return Err(StateError::InvalidTx(format!(
                    "mempool rejects ticket {}: not live on canonical state (same-block create/use is consensus-only)",
                    selector.value
                )));
            }
            Ok(DrcMempoolReservation::Ticket {
                sequence: selector.value,
            })
        }
    }
}

/// Ticket sequence minted by a pending or canonical create at ordinary nonce `N`.
pub fn drc_ticket_sequence_for_create_nonce(nonce: u64) -> Result<u64, StateError> {
    nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC ticket sequence overflow".into()))
}

/// Point lookup semantics for `agora_getDrcTicket` (canonical state only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrcTicketPointStatus {
    /// Ticket sequence is outstanding on the virtual state view.
    Live,
    /// Never created or already consumed — indistinguishable without extra history.
    Unknown,
}

pub fn lookup_drc_ticket_point(
    store: &StateStore,
    owner: &Address,
    ticket_sequence: u64,
) -> Result<DrcTicketPointStatus, StateError> {
    if *owner == Address::ZERO {
        return Err(StateError::InvalidTx("zero DRC ticket owner".into()));
    }
    let _ = NativeAssetId::DRC;
    let tickets = load_drc_account_tickets(store, owner)?;
    Ok(if tickets.contains(&ticket_sequence) {
        DrcTicketPointStatus::Live
    } else {
        DrcTicketPointStatus::Unknown
    })
}
