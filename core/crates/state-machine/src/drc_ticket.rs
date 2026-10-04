//! DRC outstanding ticket state and sequence consumption helpers.

use agora_types::{
    Address, DrcAccountSequence, DrcAccountSequenceSelector, DrcAccountTickets, DrcTicketCreateTx,
    NativeAssetId, DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT, DRC_TICKET_STATE_VERSION,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::accounts::{load_account, put_account_into, AccountJournal, AccountState};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::verify_drc_ticket_create_operation;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const DRC_TICKET_PREFIX: &[u8] = b"account/drc/tickets/";
pub const DRC_TICKET_ROOT_DOMAIN: &[u8] = b"agora-drc-ticket-root-v1";

pub fn drc_ticket_meta_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_TICKET_PREFIX.len() + owner.0.len());
    key.extend_from_slice(DRC_TICKET_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn drc_ticket_meta_keys(owner: &Address) -> Vec<Vec<u8>> {
    vec![drc_ticket_meta_key(owner)]
}

pub fn load_drc_account_tickets(
    store: &StateStore,
    owner: &Address,
) -> Result<Vec<u64>, StateError> {
    let key = drc_ticket_meta_key(owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(Vec::new());
    };
    let record = DrcAccountTickets::try_from_slice(&bytes)
        .map_err(|error| StateError::Storage(error.to_string()))?;
    record
        .validate()
        .map_err(|error| StateError::Storage(error.to_string()))?;
    if record.owner != *owner {
        return Err(StateError::Storage(
            "DRC ticket record does not match index key".into(),
        ));
    }
    Ok(record.sequences)
}

fn put_tickets_into(
    batch: &mut WriteBatch,
    owner: &Address,
    sequences: &[u64],
) -> Result<(), StateError> {
    let key = drc_ticket_meta_key(owner);
    if sequences.is_empty() {
        batch.delete_cf(ColumnFamily::Meta, &key);
        return Ok(());
    }
    let record = DrcAccountTickets {
        version: DRC_TICKET_STATE_VERSION,
        owner: *owner,
        sequences: sequences.to_vec(),
    };
    record
        .validate()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    let bytes = borsh::to_vec(&record).map_err(|error| StateError::Storage(error.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &key, &bytes);
    Ok(())
}

pub fn drc_ticket_root(store: &StateStore) -> Result<agora_types::Hash, StateError> {
    let mut entries = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, DRC_TICKET_PREFIX)? {
        if key.len() != DRC_TICKET_PREFIX.len() + 20 {
            return Err(StateError::Storage("invalid DRC ticket key length".into()));
        }
        let mut owner = [0; 20];
        owner.copy_from_slice(&key[DRC_TICKET_PREFIX.len()..]);
        let record = DrcAccountTickets::try_from_slice(&bytes)
            .map_err(|error| StateError::Storage(error.to_string()))?;
        record
            .validate()
            .map_err(|error| StateError::Storage(error.to_string()))?;
        if record.owner != agora_types::Address(owner) {
            return Err(StateError::Storage(
                "DRC ticket record does not match index key".into(),
            ));
        }
        entries.push(record);
    }
    entries.sort_by_key(|record| record.owner.0);
    Ok(agora_types::Hash::hash_borsh(&(
        DRC_TICKET_ROOT_DOMAIN,
        entries,
    )))
}

/// How an accepted DRC account operation consumes its sequence binding.
pub enum DrcSequenceConsumption {
    AdvanceNonce,
    ConsumeTicket(u64),
}

pub fn validate_drc_account_sequence(
    store: &StateStore,
    owner: &Address,
    selector: DrcAccountSequenceSelector,
) -> Result<DrcSequenceConsumption, StateError> {
    selector
        .validate()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    match selector.kind {
        DrcAccountSequence::Nonce => {
            let account = load_account(store, NativeAssetId::DRC, owner)?;
            if account.nonce != selector.value {
                return Err(StateError::InvalidTx(format!(
                    "bad DRC account nonce: got {} expected {}",
                    selector.value, account.nonce
                )));
            }
            Ok(DrcSequenceConsumption::AdvanceNonce)
        }
        DrcAccountSequence::Ticket => {
            let tickets = load_drc_account_tickets(store, owner)?;
            if !tickets.iter().any(|seq| *seq == selector.value) {
                return Err(StateError::InvalidTx(format!(
                    "unknown DRC ticket sequence {}",
                    selector.value
                )));
            }
            Ok(DrcSequenceConsumption::ConsumeTicket(selector.value))
        }
    }
}

pub fn commit_drc_sequence_consumption(
    batch: &mut WriteBatch,
    owner: &Address,
    account: &mut AccountState,
    consumption: DrcSequenceConsumption,
    tickets_before: &[u64],
) -> Result<(), StateError> {
    match consumption {
        DrcSequenceConsumption::AdvanceNonce => {
            account.nonce = account
                .nonce
                .checked_add(1)
                .ok_or_else(|| StateError::InvalidTx("DRC account nonce overflow".into()))?;
            Ok(())
        }
        DrcSequenceConsumption::ConsumeTicket(sequence) => {
            let mut next: Vec<u64> = tickets_before
                .iter()
                .copied()
                .filter(|seq| *seq != sequence)
                .collect();
            next.sort_unstable();
            put_tickets_into(batch, owner, &next)
        }
    }
}

pub fn apply_drc_ticket_create(
    store: &StateStore,
    tx: &DrcTicketCreateTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<u64, StateError> {
    verify_drc_ticket_create_operation(store, tx, auth)?;

    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    if owner.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad DRC ticket-create nonce: got {} expected {}",
            tx.nonce, owner.nonce
        )));
    }
    if owner.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC ticket-create balance".into(),
        ));
    }

    let ticket_sequence = tx
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC ticket sequence overflow".into()))?;
    let next_nonce = ticket_sequence
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC ticket-create nonce overflow".into()))?;

    let mut tickets = load_drc_account_tickets(store, &tx.owner)?;
    if tickets.iter().any(|seq| *seq == ticket_sequence) {
        return Err(StateError::InvalidTx(format!(
            "duplicate DRC ticket sequence {ticket_sequence}"
        )));
    }
    if tickets.len() >= DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT {
        return Err(StateError::InvalidTx(
            "DRC outstanding ticket cap exceeded".into(),
        ));
    }
    tickets.push(ticket_sequence);
    tickets.sort_unstable();

    journal
        .before
        .push((NativeAssetId::DRC, tx.owner, owner.clone()));
    owner.balance -= tx.fee.as_base_units();
    owner.nonce = next_nonce;
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;
    put_tickets_into(batch, &tx.owner, &tickets)?;
    Ok(ticket_sequence)
}
