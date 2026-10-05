//! Native DRC check state transitions (exact-value authorization; no create-time lock).

use agora_types::{
    check_cancel_submitter_allowed, check_cash_allowed, resolve_drc_account_sequence, Address,
    DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, DrcCheckLive, DrcCheckOutcome,
    DrcCheckReceipt, Hash, NativeAssetId, DRC_CHECK_CANCEL_TICKET_VERSION,
    DRC_CHECK_CASH_TICKET_VERSION, DRC_CHECK_CREATE_TICKET_VERSION, DRC_CHECK_LIVE_STATE_VERSION,
    DRC_CHECK_RECEIPT_VERSION, DRC_MAX_LIVE_CHECKS_PER_ACCOUNT,
};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::{
    verify_drc_check_cancel_operation, verify_drc_check_cash_operation,
    verify_drc_check_create_operation,
};
use crate::drc_deposit_preauth::load_drc_deposit_preauth;
use crate::drc_policy::load_drc_account_policy;
use crate::drc_ticket::{begin_drc_account_sequence, finish_drc_account_sequence};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const LIVE_PREFIX: &[u8] = b"check/drc/live/";
const OWNER_INDEX_PREFIX: &[u8] = b"check/drc/owner/";
const SETTLED_PREFIX: &[u8] = b"check/drc/settled/";
const RECEIPT_PREFIX: &[u8] = b"check/drc/receipt/";
pub const DRC_CHECK_ROOT_DOMAIN: &[u8] = b"agora-drc-check-root-v1";

#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct DrcCheckOwnerIndex {
    version: u32,
    owner: Address,
    live_ids: Vec<Hash>,
}

const OWNER_INDEX_VERSION: u32 = 1;

pub fn check_live_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(LIVE_PREFIX.len() + 32);
    key.extend_from_slice(LIVE_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn check_owner_index_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(OWNER_INDEX_PREFIX.len() + 20);
    key.extend_from_slice(OWNER_INDEX_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn check_settled_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(SETTLED_PREFIX.len() + 32);
    key.extend_from_slice(SETTLED_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn check_receipt_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(RECEIPT_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn check_meta_keys_for_create(tx: &DrcCheckCreateTx) -> Vec<Vec<u8>> {
    let id = tx.check_id();
    vec![
        check_live_key(&id),
        check_owner_index_key(&tx.owner),
        check_settled_key(&id),
    ]
}

pub fn check_meta_keys_for_settlement(id: &Hash, owner: &Address) -> Vec<Vec<u8>> {
    vec![
        check_live_key(id),
        check_owner_index_key(owner),
        check_settled_key(id),
        check_receipt_key(id),
    ]
}

pub fn load_drc_check_live(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcCheckLive>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &check_live_key(id))? else {
        return Ok(None);
    };
    let live =
        DrcCheckLive::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if live.check_id != *id {
        return Err(StateError::Storage(
            "DRC check live id does not match index key".into(),
        ));
    }
    Ok(Some(live))
}

pub fn load_drc_check_receipt(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcCheckReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &check_receipt_key(id))? else {
        return Ok(None);
    };
    let receipt =
        DrcCheckReceipt::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    receipt
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if receipt.check_id != *id {
        return Err(StateError::Storage(
            "DRC check receipt id does not match index key".into(),
        ));
    }
    Ok(Some(receipt))
}

fn load_owner_index(store: &StateStore, owner: &Address) -> Result<Vec<Hash>, StateError> {
    let key = check_owner_index_key(owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(Vec::new());
    };
    let record = DrcCheckOwnerIndex::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if record.version != OWNER_INDEX_VERSION || record.owner != *owner {
        return Err(StateError::Storage("invalid DRC check owner index".into()));
    }
    Ok(record.live_ids)
}

fn put_owner_index(
    batch: &mut WriteBatch,
    owner: &Address,
    live_ids: &[Hash],
) -> Result<(), StateError> {
    let key = check_owner_index_key(owner);
    if live_ids.is_empty() {
        batch.delete_cf(ColumnFamily::Meta, &key);
        return Ok(());
    }
    if live_ids.len() > DRC_MAX_LIVE_CHECKS_PER_ACCOUNT {
        return Err(StateError::InvalidTx("DRC live check cap exceeded".into()));
    }
    let record = DrcCheckOwnerIndex {
        version: OWNER_INDEX_VERSION,
        owner: *owner,
        live_ids: live_ids.to_vec(),
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &key,
        &borsh::to_vec(&record).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn drc_check_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries: Vec<DrcCheckLive> = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, LIVE_PREFIX)? {
        if key.len() != LIVE_PREFIX.len() + 32 {
            return Err(StateError::Storage(
                "invalid DRC check live key length".into(),
            ));
        }
        let live =
            DrcCheckLive::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
        live.validate()
            .map_err(|e| StateError::Storage(e.to_string()))?;
        entries.push(live);
    }
    entries.sort_by_key(|e| e.check_id.as_bytes().to_vec());
    Ok(Hash::hash_borsh(&(DRC_CHECK_ROOT_DOMAIN, entries)))
}

fn ensure_unsettled(store: &StateStore, id: &Hash) -> Result<(), StateError> {
    if store
        .get_cf(ColumnFamily::Meta, &check_settled_key(id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("DRC check already settled".into()));
    }
    Ok(())
}

pub fn apply_drc_check_create(
    store: &StateStore,
    tx: &DrcCheckCreateTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<Hash, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_check_create_operation(store, tx, auth)?;

    let check_id = tx.check_id();
    if store
        .get_cf(ColumnFamily::Meta, &check_live_key(&check_id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("duplicate DRC check id".into()));
    }
    ensure_unsettled(store, &check_id)?;

    let destination_policy = load_drc_account_policy(store, &tx.destination)?;
    if destination_policy.require_destination_tag && tx.authenticated_destination_tag().is_none() {
        return Err(StateError::InvalidTx(
            "DRC destination tag required by destination policy".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    let sequence_ctx = if tx.version >= DRC_CHECK_CREATE_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_CHECK_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.owner, selector)?)
    } else {
        if owner.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC check-create nonce: got {} expected {}",
                tx.nonce, owner.nonce
            )));
        }
        None
    };

    if owner.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC check-create balance for fee".into(),
        ));
    }

    let mut live_ids = load_owner_index(store, &tx.owner)?;
    if live_ids.len() >= DRC_MAX_LIVE_CHECKS_PER_ACCOUNT {
        return Err(StateError::InvalidTx("DRC live check cap exceeded".into()));
    }
    if live_ids.contains(&check_id) {
        return Err(StateError::InvalidTx("duplicate DRC check id".into()));
    }

    journal
        .before
        .push((NativeAssetId::DRC, tx.owner, owner.clone()));
    owner.balance -= tx.fee.as_base_units();
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
            .ok_or_else(|| StateError::InvalidTx("DRC check-create nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;

    live_ids.push(check_id);
    live_ids.sort_by_key(|id| id.as_bytes().to_vec());
    put_owner_index(batch, &tx.owner, &live_ids)?;

    let live = DrcCheckLive {
        version: DRC_CHECK_LIVE_STATE_VERSION,
        check_id,
        owner: tx.owner,
        destination: tx.destination,
        amount: tx.amount,
        destination_tag: tx.destination_tag,
        source_tag: tx.source_tag,
        invoice_id: tx.invoice_id,
        expires_after_blue_score: tx.expires_after_blue_score,
        create_blue_score: application_blue_score,
    };
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &check_live_key(&check_id),
        &borsh::to_vec(&live).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(check_id)
}

pub fn apply_drc_check_cash(
    store: &StateStore,
    tx: &DrcCheckCashTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcCheckReceipt, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_check_cash_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.check_id)?;
    let live = load_drc_check_live(store, &tx.check_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown DRC check".into()))?;

    if tx.submitter != live.destination {
        return Err(StateError::InvalidTx(
            "DRC check cash submitter must be destination".into(),
        ));
    }

    if !check_cash_allowed(application_blue_score, live.expires_after_blue_score) {
        return Err(StateError::InvalidTx(
            "DRC check expired at this blue score".into(),
        ));
    }

    let destination_policy = load_drc_account_policy(store, &live.destination)?;
    if destination_policy.deposit_auth_required
        && live.owner != live.destination
        && !load_drc_deposit_preauth(store, &live.destination, &live.owner)?
    {
        return Err(StateError::InvalidTx(
            "DRC deposit authorization required by destination policy".into(),
        ));
    }

    let mut submitter = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
    let sequence_ctx = if tx.version >= DRC_CHECK_CASH_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_CHECK_CASH_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if submitter.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC check-cash nonce: got {} expected {}",
                tx.nonce, submitter.nonce
            )));
        }
        None
    };

    if submitter.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC check-cash submitter balance for fee".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &live.owner)?;
    if owner.balance < live.amount.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC check owner balance for cash".into(),
        ));
    }

    let mut destination = load_account(store, NativeAssetId::DRC, &live.destination)?;
    let new_destination_balance = destination
        .balance
        .checked_add(live.amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("DRC check cash destination overflow".into()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.submitter, submitter.clone()));
    journal
        .before
        .push((NativeAssetId::DRC, live.owner, owner.clone()));
    journal
        .before
        .push((NativeAssetId::DRC, live.destination, destination.clone()));

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
            .ok_or_else(|| StateError::InvalidTx("DRC check-cash nonce overflow".into()))?;
    }
    owner.balance -= live.amount.as_base_units();
    destination.balance = new_destination_balance;

    put_account_into(batch, NativeAssetId::DRC, &tx.submitter, &submitter)?;
    put_account_into(batch, NativeAssetId::DRC, &live.owner, &owner)?;
    put_account_into(batch, NativeAssetId::DRC, &live.destination, &destination)?;

    settle_check(
        store,
        batch,
        &live,
        DrcCheckOutcome::Cashed,
        application_blue_score,
        tx.cash_tx_id(),
    )?;

    Ok(DrcCheckReceipt {
        version: DRC_CHECK_RECEIPT_VERSION,
        check_id: live.check_id,
        outcome: DrcCheckOutcome::Cashed,
        owner: live.owner,
        destination: live.destination,
        amount: live.amount,
        destination_tag: live.destination_tag,
        source_tag: live.source_tag,
        invoice_id: live.invoice_id,
        settlement_blue_score: application_blue_score,
        settlement_tx_id: tx.cash_tx_id(),
    })
}

pub fn apply_drc_check_cancel(
    store: &StateStore,
    tx: &DrcCheckCancelTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcCheckReceipt, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_check_cancel_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.check_id)?;
    let live = load_drc_check_live(store, &tx.check_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown DRC check".into()))?;

    if !check_cancel_submitter_allowed(
        application_blue_score,
        live.expires_after_blue_score,
        tx.submitter,
        live.owner,
        live.destination,
    ) {
        return Err(StateError::InvalidTx(
            "DRC check cancel submitter not authorized".into(),
        ));
    }

    let mut submitter = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
    let sequence_ctx = if tx.version >= DRC_CHECK_CANCEL_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_CHECK_CANCEL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if submitter.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC check-cancel nonce: got {} expected {}",
                tx.nonce, submitter.nonce
            )));
        }
        None
    };

    if submitter.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC check-cancel balance".into(),
        ));
    }

    journal
        .before
        .push((NativeAssetId::DRC, tx.submitter, submitter.clone()));

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
            .ok_or_else(|| StateError::InvalidTx("DRC check-cancel nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &tx.submitter, &submitter)?;

    settle_check(
        store,
        batch,
        &live,
        DrcCheckOutcome::Cancelled,
        application_blue_score,
        tx.cancel_tx_id(),
    )?;

    Ok(DrcCheckReceipt {
        version: DRC_CHECK_RECEIPT_VERSION,
        check_id: live.check_id,
        outcome: DrcCheckOutcome::Cancelled,
        owner: live.owner,
        destination: live.destination,
        amount: live.amount,
        destination_tag: live.destination_tag,
        source_tag: live.source_tag,
        invoice_id: live.invoice_id,
        settlement_blue_score: application_blue_score,
        settlement_tx_id: tx.cancel_tx_id(),
    })
}

fn settle_check(
    store: &StateStore,
    batch: &mut WriteBatch,
    live: &DrcCheckLive,
    outcome: DrcCheckOutcome,
    score: u64,
    settlement_tx_id: Hash,
) -> Result<(), StateError> {
    let mut live_ids = load_owner_index(store, &live.owner)?;
    live_ids.retain(|id| *id != live.check_id);
    put_owner_index(batch, &live.owner, &live_ids)?;

    batch.delete_cf(ColumnFamily::Meta, &check_live_key(&live.check_id));
    batch.put_cf(
        ColumnFamily::Meta,
        &check_settled_key(&live.check_id),
        settlement_tx_id.as_bytes(),
    );

    let receipt = DrcCheckReceipt {
        version: DRC_CHECK_RECEIPT_VERSION,
        check_id: live.check_id,
        outcome,
        owner: live.owner,
        destination: live.destination,
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
        &check_receipt_key(&live.check_id),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn lookup_drc_check_point(
    store: &StateStore,
    check_id: &Hash,
) -> Result<&'static str, StateError> {
    if *check_id == Hash::ZERO {
        return Err(StateError::InvalidTx("zero DRC check id".into()));
    }
    if load_drc_check_live(store, check_id)?.is_some() {
        return Ok("live");
    }
    Ok("unknown")
}
