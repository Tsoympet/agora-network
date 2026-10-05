//! Native DRC escrow state transitions (typed, contract-free).

use agora_types::{
    escrow_cancel_allowed, escrow_finish_allowed, resolve_drc_account_sequence, Address,
    DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx, DrcEscrowLive, DrcEscrowOutcome,
    DrcEscrowReceipt, Hash, NativeAssetId, DRC_ESCROW_CANCEL_TICKET_VERSION,
    DRC_ESCROW_CREATE_TICKET_VERSION, DRC_ESCROW_FINISH_TICKET_VERSION,
    DRC_ESCROW_LIVE_STATE_VERSION, DRC_ESCROW_RECEIPT_VERSION, DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT,
};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::{
    verify_drc_escrow_cancel_operation, verify_drc_escrow_create_operation,
    verify_drc_escrow_finish_operation,
};
use crate::drc_deposit_preauth::load_drc_deposit_preauth;
use crate::drc_policy::load_drc_account_policy;
use crate::drc_ticket::{begin_drc_account_sequence, finish_drc_account_sequence};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const LIVE_PREFIX: &[u8] = b"escrow/drc/live/";
const OWNER_INDEX_PREFIX: &[u8] = b"escrow/drc/owner/";
const SETTLED_PREFIX: &[u8] = b"escrow/drc/settled/";
const RECEIPT_PREFIX: &[u8] = b"escrow/drc/receipt/";
pub const DRC_ESCROW_ROOT_DOMAIN: &[u8] = b"agora-drc-escrow-root-v1";

#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct DrcEscrowOwnerIndex {
    version: u32,
    owner: Address,
    live_ids: Vec<Hash>,
}

const OWNER_INDEX_VERSION: u32 = 1;

pub fn escrow_live_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(LIVE_PREFIX.len() + 32);
    key.extend_from_slice(LIVE_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn escrow_owner_index_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(OWNER_INDEX_PREFIX.len() + 20);
    key.extend_from_slice(OWNER_INDEX_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn escrow_settled_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(SETTLED_PREFIX.len() + 32);
    key.extend_from_slice(SETTLED_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn escrow_receipt_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(RECEIPT_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn escrow_meta_keys_for_create(tx: &DrcEscrowCreateTx) -> Vec<Vec<u8>> {
    let id = tx.escrow_id();
    vec![
        escrow_live_key(&id),
        escrow_owner_index_key(&tx.owner),
        escrow_settled_key(&id),
    ]
}

pub fn escrow_meta_keys_for_settlement(id: &Hash, owner: &Address) -> Vec<Vec<u8>> {
    vec![
        escrow_live_key(id),
        escrow_owner_index_key(owner),
        escrow_settled_key(id),
        escrow_receipt_key(id),
    ]
}

pub fn load_drc_escrow_live(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcEscrowLive>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &escrow_live_key(id))? else {
        return Ok(None);
    };
    let live =
        DrcEscrowLive::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if live.escrow_id != *id {
        return Err(StateError::Storage(
            "DRC escrow live id does not match index key".into(),
        ));
    }
    Ok(Some(live))
}

pub fn load_drc_escrow_receipt(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcEscrowReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &escrow_receipt_key(id))? else {
        return Ok(None);
    };
    let receipt =
        DrcEscrowReceipt::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    receipt
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if receipt.escrow_id != *id {
        return Err(StateError::Storage(
            "DRC escrow receipt id does not match index key".into(),
        ));
    }
    Ok(Some(receipt))
}

fn load_owner_index(store: &StateStore, owner: &Address) -> Result<Vec<Hash>, StateError> {
    let key = escrow_owner_index_key(owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(Vec::new());
    };
    let record = DrcEscrowOwnerIndex::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if record.version != OWNER_INDEX_VERSION || record.owner != *owner {
        return Err(StateError::Storage("invalid DRC escrow owner index".into()));
    }
    Ok(record.live_ids)
}

fn put_owner_index(
    batch: &mut WriteBatch,
    owner: &Address,
    live_ids: &[Hash],
) -> Result<(), StateError> {
    let key = escrow_owner_index_key(owner);
    if live_ids.is_empty() {
        batch.delete_cf(ColumnFamily::Meta, &key);
        return Ok(());
    }
    if live_ids.len() > DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT {
        return Err(StateError::InvalidTx("DRC live escrow cap exceeded".into()));
    }
    let record = DrcEscrowOwnerIndex {
        version: OWNER_INDEX_VERSION,
        owner: *owner,
        live_ids: live_ids.to_vec(),
    };
    let bytes = borsh::to_vec(&record).map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &key, &bytes);
    Ok(())
}

pub fn drc_escrow_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries: Vec<DrcEscrowLive> = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, LIVE_PREFIX)? {
        if key.len() != LIVE_PREFIX.len() + 32 {
            return Err(StateError::Storage(
                "invalid DRC escrow live key length".into(),
            ));
        }
        let live = DrcEscrowLive::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        live.validate()
            .map_err(|e| StateError::Storage(e.to_string()))?;
        entries.push(live);
    }
    entries.sort_by_key(|e| e.escrow_id.as_bytes().to_vec());
    Ok(Hash::hash_borsh(&(DRC_ESCROW_ROOT_DOMAIN, entries)))
}

fn ensure_unsettled(store: &StateStore, id: &Hash) -> Result<(), StateError> {
    if store
        .get_cf(ColumnFamily::Meta, &escrow_settled_key(id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("DRC escrow already settled".into()));
    }
    Ok(())
}

pub fn apply_drc_escrow_create(
    store: &StateStore,
    tx: &DrcEscrowCreateTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<Hash, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_escrow_create_operation(store, tx, auth)?;

    let escrow_id = tx.escrow_id();
    if store
        .get_cf(ColumnFamily::Meta, &escrow_live_key(&escrow_id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("duplicate DRC escrow id".into()));
    }
    ensure_unsettled(store, &escrow_id)?;

    let recipient_policy = load_drc_account_policy(store, &tx.recipient)?;
    if recipient_policy.require_destination_tag && tx.authenticated_destination_tag().is_none() {
        return Err(StateError::InvalidTx(
            "DRC destination tag required by recipient policy".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    let sequence_ctx = if tx.version >= DRC_ESCROW_CREATE_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_ESCROW_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.owner, selector)?)
    } else {
        if owner.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC escrow-create nonce: got {} expected {}",
                tx.nonce, owner.nonce
            )));
        }
        None
    };

    let debit = tx
        .amount
        .as_base_units()
        .checked_add(tx.fee.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("DRC escrow create amount+fee overflow".into()))?;
    if owner.balance < debit {
        return Err(StateError::InvalidTx(
            "insufficient DRC escrow-create balance".into(),
        ));
    }

    let mut live_ids = load_owner_index(store, &tx.owner)?;
    if live_ids.len() >= DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT {
        return Err(StateError::InvalidTx("DRC live escrow cap exceeded".into()));
    }
    if live_ids.contains(&escrow_id) {
        return Err(StateError::InvalidTx("duplicate DRC escrow id".into()));
    }

    journal
        .before
        .push((NativeAssetId::DRC, tx.owner, owner.clone()));
    owner.balance -= debit;
    if let Some(ctx) = sequence_ctx {
        finish_drc_account_sequence(
            batch,
            &tx.owner,
            &mut owner,
            ctx.consumption,
            &ctx.tickets_before,
        )?;
    } else {
        owner.nonce = owner
            .nonce
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("DRC escrow-create nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;

    live_ids.push(escrow_id);
    live_ids.sort_by_key(|id| id.as_bytes().to_vec());
    put_owner_index(batch, &tx.owner, &live_ids)?;

    let live = DrcEscrowLive {
        version: DRC_ESCROW_LIVE_STATE_VERSION,
        escrow_id,
        owner: tx.owner,
        recipient: tx.recipient,
        amount: tx.amount,
        destination_tag: tx.destination_tag,
        source_tag: tx.source_tag,
        invoice_id: tx.invoice_id,
        finish_after_blue_score: tx.finish_after_blue_score,
        cancel_after_blue_score: tx.cancel_after_blue_score,
        create_blue_score: application_blue_score,
    };
    live.validate()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &escrow_live_key(&escrow_id),
        &borsh::to_vec(&live).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(escrow_id)
}

pub fn apply_drc_escrow_finish(
    store: &StateStore,
    tx: &DrcEscrowFinishTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcEscrowReceipt, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_escrow_finish_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.escrow_id)?;
    let live = load_drc_escrow_live(store, &tx.escrow_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown DRC escrow".into()))?;

    if !escrow_finish_allowed(
        application_blue_score,
        live.finish_after_blue_score,
        live.cancel_after_blue_score,
    ) {
        return Err(StateError::InvalidTx(
            "DRC escrow finish not yet allowed at this blue score".into(),
        ));
    }

    let recipient_policy = load_drc_account_policy(store, &live.recipient)?;
    if recipient_policy.deposit_auth_required
        && live.owner != live.recipient
        && !load_drc_deposit_preauth(store, &live.recipient, &live.owner)?
    {
        return Err(StateError::InvalidTx(
            "DRC deposit authorization required by recipient policy".into(),
        ));
    }

    let mut submitter = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
    let sequence_ctx = if tx.version >= DRC_ESCROW_FINISH_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_ESCROW_FINISH_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if submitter.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC escrow-finish nonce: got {} expected {}",
                tx.nonce, submitter.nonce
            )));
        }
        None
    };

    if submitter.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC escrow-finish balance".into(),
        ));
    }

    let mut recipient = load_account(store, NativeAssetId::DRC, &live.recipient)?;
    let new_recipient_balance = recipient
        .balance
        .checked_add(live.amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("DRC escrow finish recipient overflow".into()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.submitter, submitter.clone()));
    journal
        .before
        .push((NativeAssetId::DRC, live.recipient, recipient.clone()));

    submitter.balance -= tx.fee.as_base_units();
    if let Some(ctx) = sequence_ctx {
        finish_drc_account_sequence(
            batch,
            &tx.submitter,
            &mut submitter,
            ctx.consumption,
            &ctx.tickets_before,
        )?;
    } else {
        submitter.nonce = submitter
            .nonce
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("DRC escrow-finish nonce overflow".into()))?;
    }
    recipient.balance = new_recipient_balance;
    put_account_into(batch, NativeAssetId::DRC, &tx.submitter, &submitter)?;
    put_account_into(batch, NativeAssetId::DRC, &live.recipient, &recipient)?;

    settle_escrow(
        store,
        batch,
        &live,
        DrcEscrowOutcome::Finished,
        application_blue_score,
        tx.finish_tx_id(),
    )?;

    Ok(DrcEscrowReceipt {
        version: DRC_ESCROW_RECEIPT_VERSION,
        escrow_id: live.escrow_id,
        outcome: DrcEscrowOutcome::Finished,
        owner: live.owner,
        recipient: live.recipient,
        amount: live.amount,
        destination_tag: live.destination_tag,
        source_tag: live.source_tag,
        invoice_id: live.invoice_id,
        settlement_blue_score: application_blue_score,
        settlement_tx_id: tx.finish_tx_id(),
    })
}

pub fn apply_drc_escrow_cancel(
    store: &StateStore,
    tx: &DrcEscrowCancelTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcEscrowReceipt, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_escrow_cancel_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.escrow_id)?;
    let live = load_drc_escrow_live(store, &tx.escrow_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown DRC escrow".into()))?;

    if !escrow_cancel_allowed(application_blue_score, live.cancel_after_blue_score) {
        return Err(StateError::InvalidTx(
            "DRC escrow cancel not yet allowed at this blue score".into(),
        ));
    }

    let mut submitter = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
    let sequence_ctx = if tx.version >= DRC_ESCROW_CANCEL_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_ESCROW_CANCEL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if submitter.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC escrow-cancel nonce: got {} expected {}",
                tx.nonce, submitter.nonce
            )));
        }
        None
    };

    if submitter.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC escrow-cancel balance".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &live.owner)?;
    let new_owner_balance = owner
        .balance
        .checked_add(live.amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("DRC escrow cancel owner balance overflow".into()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.submitter, submitter.clone()));
    journal
        .before
        .push((NativeAssetId::DRC, live.owner, owner.clone()));

    submitter.balance -= tx.fee.as_base_units();
    if let Some(ctx) = sequence_ctx {
        finish_drc_account_sequence(
            batch,
            &tx.submitter,
            &mut submitter,
            ctx.consumption,
            &ctx.tickets_before,
        )?;
    } else {
        submitter.nonce = submitter
            .nonce
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("DRC escrow-cancel nonce overflow".into()))?;
    }
    owner.balance = new_owner_balance;
    put_account_into(batch, NativeAssetId::DRC, &tx.submitter, &submitter)?;
    put_account_into(batch, NativeAssetId::DRC, &live.owner, &owner)?;

    settle_escrow(
        store,
        batch,
        &live,
        DrcEscrowOutcome::Cancelled,
        application_blue_score,
        tx.cancel_tx_id(),
    )?;

    Ok(DrcEscrowReceipt {
        version: DRC_ESCROW_RECEIPT_VERSION,
        escrow_id: live.escrow_id,
        outcome: DrcEscrowOutcome::Cancelled,
        owner: live.owner,
        recipient: live.recipient,
        amount: live.amount,
        destination_tag: live.destination_tag,
        source_tag: live.source_tag,
        invoice_id: live.invoice_id,
        settlement_blue_score: application_blue_score,
        settlement_tx_id: tx.cancel_tx_id(),
    })
}

fn settle_escrow(
    store: &StateStore,
    batch: &mut WriteBatch,
    live: &DrcEscrowLive,
    outcome: DrcEscrowOutcome,
    score: u64,
    settlement_tx_id: Hash,
) -> Result<(), StateError> {
    let mut live_ids = load_owner_index(store, &live.owner)?;
    live_ids.retain(|id| *id != live.escrow_id);
    put_owner_index(batch, &live.owner, &live_ids)?;

    batch.delete_cf(ColumnFamily::Meta, &escrow_live_key(&live.escrow_id));
    batch.put_cf(
        ColumnFamily::Meta,
        &escrow_settled_key(&live.escrow_id),
        settlement_tx_id.as_bytes(),
    );

    let receipt = DrcEscrowReceipt {
        version: DRC_ESCROW_RECEIPT_VERSION,
        escrow_id: live.escrow_id,
        outcome,
        owner: live.owner,
        recipient: live.recipient,
        amount: live.amount,
        destination_tag: live.destination_tag,
        source_tag: live.source_tag,
        invoice_id: live.invoice_id,
        settlement_blue_score: score,
        settlement_tx_id,
    };
    receipt
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &escrow_receipt_key(&live.escrow_id),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn lookup_drc_escrow_point(
    store: &StateStore,
    escrow_id: &Hash,
) -> Result<&'static str, StateError> {
    if *escrow_id == Hash::ZERO {
        return Err(StateError::InvalidTx("zero DRC escrow id".into()));
    }
    if load_drc_escrow_live(store, escrow_id)?.is_some() {
        return Ok("live");
    }
    Ok("unknown")
}
