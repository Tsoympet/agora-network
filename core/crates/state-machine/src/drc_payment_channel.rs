//! Native DRC payment channel state transitions (locked DRC + cumulative off-ledger claims).

use agora_types::{
    payment_channel_cancel_after_valid_at_create, payment_channel_claim_submitter_allowed,
    payment_channel_close_submitter_allowed, payment_channel_finalize_allowed,
    payment_channel_fund_submitter_allowed, payment_channel_onchain_claim_allowed,
    payment_channel_owner_schedule_deadline, resolve_drc_account_sequence, Address, Amount,
    DrcPaymentChannelClaimEvent, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundEvent,
    DrcPaymentChannelFundTx, DrcPaymentChannelLive, DrcPaymentChannelOutcome,
    DrcPaymentChannelReceipt, DrcPaymentChannelScheduleEvent, Hash, NativeAssetId,
    DRC_MAX_LIVE_PAYMENT_CHANNELS_PER_ACCOUNT, DRC_PAYMENT_CHANNEL_CLAIM_EVENT_VERSION,
    DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION, DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION,
    DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION, DRC_PAYMENT_CHANNEL_FUND_EVENT_VERSION,
    DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION, DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION,
    DRC_PAYMENT_CHANNEL_RECEIPT_VERSION, DRC_PAYMENT_CHANNEL_SCHEDULE_EVENT_VERSION,
};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::{
    verify_drc_payment_channel_claim_operation, verify_drc_payment_channel_close_operation,
    verify_drc_payment_channel_create_operation, verify_drc_payment_channel_fund_operation,
};
use crate::drc_policy::load_drc_account_policy;
use crate::drc_ticket::{begin_drc_account_sequence, finish_drc_account_sequence};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const LIVE_PREFIX: &[u8] = b"paychan/drc/live/";
const OWNER_INDEX_PREFIX: &[u8] = b"paychan/drc/owner/";
const SETTLED_PREFIX: &[u8] = b"paychan/drc/settled/";
const RECEIPT_PREFIX: &[u8] = b"paychan/drc/receipt/";
const FUND_EVENT_PREFIX: &[u8] = b"paychan/drc/fund/";
const CLAIM_EVENT_PREFIX: &[u8] = b"paychan/drc/claim/";
const SCHEDULE_EVENT_PREFIX: &[u8] = b"paychan/drc/schedule/";
pub const DRC_PAYMENT_CHANNEL_ROOT_DOMAIN: &[u8] = b"agora-drc-payment-channel-root-v1";

#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct OwnerIndex {
    version: u32,
    owner: Address,
    live_ids: Vec<Hash>,
}

const OWNER_INDEX_VERSION: u32 = 1;

pub fn payment_channel_live_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(LIVE_PREFIX.len() + 32);
    key.extend_from_slice(LIVE_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn payment_channel_owner_index_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(OWNER_INDEX_PREFIX.len() + 20);
    key.extend_from_slice(OWNER_INDEX_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn payment_channel_settled_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(SETTLED_PREFIX.len() + 32);
    key.extend_from_slice(SETTLED_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn payment_channel_receipt_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(RECEIPT_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn payment_channel_fund_event_key(fund_tx_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(FUND_EVENT_PREFIX.len() + 32);
    key.extend_from_slice(FUND_EVENT_PREFIX);
    key.extend_from_slice(fund_tx_id.as_bytes());
    key
}

pub fn payment_channel_claim_event_key(claim_tx_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(CLAIM_EVENT_PREFIX.len() + 32);
    key.extend_from_slice(CLAIM_EVENT_PREFIX);
    key.extend_from_slice(claim_tx_id.as_bytes());
    key
}

pub fn payment_channel_schedule_event_key(close_tx_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(SCHEDULE_EVENT_PREFIX.len() + 32);
    key.extend_from_slice(SCHEDULE_EVENT_PREFIX);
    key.extend_from_slice(close_tx_id.as_bytes());
    key
}

pub fn payment_channel_meta_keys_for_fund(
    channel_id: &Hash,
    owner: &Address,
    fund_tx_id: &Hash,
) -> Vec<Vec<u8>> {
    let mut keys = payment_channel_meta_keys_for_mutating(channel_id, owner);
    keys.push(payment_channel_fund_event_key(fund_tx_id));
    keys
}

pub fn payment_channel_meta_keys_for_claim(
    channel_id: &Hash,
    owner: &Address,
    claim_tx_id: &Hash,
) -> Vec<Vec<u8>> {
    let mut keys = payment_channel_meta_keys_for_mutating(channel_id, owner);
    keys.push(payment_channel_claim_event_key(claim_tx_id));
    keys
}

pub fn payment_channel_meta_keys_for_schedule_close(
    channel_id: &Hash,
    owner: &Address,
    close_tx_id: &Hash,
) -> Vec<Vec<u8>> {
    let mut keys = payment_channel_meta_keys_for_mutating(channel_id, owner);
    keys.push(payment_channel_schedule_event_key(close_tx_id));
    keys
}

pub fn payment_channel_meta_keys_for_create(tx: &DrcPaymentChannelCreateTx) -> Vec<Vec<u8>> {
    let id = tx.channel_id();
    vec![
        payment_channel_live_key(&id),
        payment_channel_owner_index_key(&tx.owner),
        payment_channel_settled_key(&id),
    ]
}

pub fn payment_channel_meta_keys_for_mutating(id: &Hash, owner: &Address) -> Vec<Vec<u8>> {
    vec![
        payment_channel_live_key(id),
        payment_channel_owner_index_key(owner),
        payment_channel_settled_key(id),
        payment_channel_receipt_key(id),
    ]
}

pub fn load_drc_payment_channel_live(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcPaymentChannelLive>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &payment_channel_live_key(id))? else {
        return Ok(None);
    };
    let live = DrcPaymentChannelLive::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if live.channel_id != *id {
        return Err(StateError::Storage(
            "DRC payment channel live id mismatch".into(),
        ));
    }
    Ok(Some(live))
}

pub fn load_drc_payment_channel_fund_event(
    store: &StateStore,
    fund_tx_id: &Hash,
) -> Result<Option<DrcPaymentChannelFundEvent>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &payment_channel_fund_event_key(fund_tx_id),
    )?
    else {
        return Ok(None);
    };
    let event = DrcPaymentChannelFundEvent::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    event
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(event))
}

pub fn load_drc_payment_channel_claim_event(
    store: &StateStore,
    claim_tx_id: &Hash,
) -> Result<Option<DrcPaymentChannelClaimEvent>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &payment_channel_claim_event_key(claim_tx_id),
    )?
    else {
        return Ok(None);
    };
    let event = DrcPaymentChannelClaimEvent::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    event
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(event))
}

pub fn load_drc_payment_channel_schedule_event(
    store: &StateStore,
    close_tx_id: &Hash,
) -> Result<Option<DrcPaymentChannelScheduleEvent>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &payment_channel_schedule_event_key(close_tx_id),
    )?
    else {
        return Ok(None);
    };
    let event = DrcPaymentChannelScheduleEvent::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    event
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(event))
}

pub fn load_drc_payment_channel_receipt(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcPaymentChannelReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &payment_channel_receipt_key(id))? else {
        return Ok(None);
    };
    let receipt = DrcPaymentChannelReceipt::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    receipt
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if receipt.channel_id != *id {
        return Err(StateError::Storage(
            "Drc payment channel receipt id mismatch".into(),
        ));
    }
    Ok(Some(receipt))
}

fn load_owner_index(store: &StateStore, owner: &Address) -> Result<Vec<Hash>, StateError> {
    let key = payment_channel_owner_index_key(owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(Vec::new());
    };
    let record =
        OwnerIndex::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if record.version != OWNER_INDEX_VERSION || record.owner != *owner {
        return Err(StateError::Storage(
            "invalid payment channel owner index".into(),
        ));
    }
    Ok(record.live_ids)
}

fn put_owner_index(
    batch: &mut WriteBatch,
    owner: &Address,
    live_ids: &[Hash],
) -> Result<(), StateError> {
    let key = payment_channel_owner_index_key(owner);
    if live_ids.is_empty() {
        batch.delete_cf(ColumnFamily::Meta, &key);
        return Ok(());
    }
    if live_ids.len() > DRC_MAX_LIVE_PAYMENT_CHANNELS_PER_ACCOUNT {
        return Err(StateError::InvalidTx(
            "DRC live payment channel cap exceeded".into(),
        ));
    }
    let record = OwnerIndex {
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

pub fn drc_payment_channel_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries: Vec<DrcPaymentChannelLive> = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, LIVE_PREFIX)? {
        if key.len() != LIVE_PREFIX.len() + 32 {
            return Err(StateError::Storage(
                "invalid payment channel live key length".into(),
            ));
        }
        let live = DrcPaymentChannelLive::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        live.validate()
            .map_err(|e| StateError::Storage(e.to_string()))?;
        entries.push(live);
    }
    entries.sort_by_key(|e| e.channel_id.as_bytes().to_vec());
    Ok(Hash::hash_borsh(&(
        DRC_PAYMENT_CHANNEL_ROOT_DOMAIN,
        entries,
    )))
}

fn ensure_unsettled(store: &StateStore, id: &Hash) -> Result<(), StateError> {
    if store
        .get_cf(ColumnFamily::Meta, &payment_channel_settled_key(id))?
        .is_some()
    {
        return Err(StateError::InvalidTx(
            "DRC payment channel already settled".into(),
        ));
    }
    Ok(())
}

fn put_live(batch: &mut WriteBatch, live: &DrcPaymentChannelLive) -> Result<(), StateError> {
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &payment_channel_live_key(&live.channel_id),
        &borsh::to_vec(live).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

fn finalize_channel(
    store: &StateStore,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
    live: &DrcPaymentChannelLive,
    application_blue_score: u64,
    settlement_tx_id: Hash,
) -> Result<DrcPaymentChannelReceipt, StateError> {
    let remainder = live
        .locked_remainder()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    let mut owner = load_account(store, NativeAssetId::DRC, &live.owner)?;
    if remainder.as_base_units() > 0 {
        journal
            .before
            .push((NativeAssetId::DRC, live.owner, owner.clone()));
        owner.balance = owner
            .balance
            .checked_add(remainder.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("payment channel remainder overflow".into()))?;
        put_account_into(batch, NativeAssetId::DRC, &live.owner, &owner)?;
    }

    let mut live_ids = load_owner_index(store, &live.owner)?;
    live_ids.retain(|id| *id != live.channel_id);
    put_owner_index(batch, &live.owner, &live_ids)?;

    batch.delete_cf(
        ColumnFamily::Meta,
        &payment_channel_live_key(&live.channel_id),
    );
    batch.put_cf(
        ColumnFamily::Meta,
        &payment_channel_settled_key(&live.channel_id),
        settlement_tx_id.as_bytes(),
    );

    let receipt = DrcPaymentChannelReceipt {
        version: DRC_PAYMENT_CHANNEL_RECEIPT_VERSION,
        channel_id: live.channel_id,
        outcome: DrcPaymentChannelOutcome::Closed,
        owner: live.owner,
        destination: live.destination,
        destination_tag: live.destination_tag,
        source_tag: live.source_tag,
        total_funded: live.total_funded,
        cumulative_claimed: live.cumulative_claimed,
        settlement_blue_score: application_blue_score,
        settlement_tx_id,
    };
    receipt
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &payment_channel_receipt_key(&live.channel_id),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(receipt)
}

pub fn apply_drc_payment_channel_create(
    store: &StateStore,
    tx: &DrcPaymentChannelCreateTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<Hash, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_payment_channel_create_operation(store, tx, auth)?;

    let channel_id = tx.channel_id();
    if store
        .get_cf(ColumnFamily::Meta, &payment_channel_live_key(&channel_id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("duplicate payment channel id".into()));
    }
    ensure_unsettled(store, &channel_id)?;

    payment_channel_cancel_after_valid_at_create(
        application_blue_score,
        tx.cancel_after_blue_score,
    )
    .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    payment_channel_owner_schedule_deadline(application_blue_score, tx.settle_delay_blue_scores)
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;

    let recipient_policy = load_drc_account_policy(store, &tx.destination)?;
    if recipient_policy.require_destination_tag && tx.authenticated_destination_tag().is_none() {
        return Err(StateError::InvalidTx(
            "DRC destination tag required by recipient policy".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    let sequence_ctx = if tx.version >= DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.owner, selector)?)
    } else {
        if owner.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad payment channel create nonce: got {} expected {}",
                tx.nonce, owner.nonce
            )));
        }
        None
    };

    let debit = tx
        .amount
        .as_base_units()
        .checked_add(tx.fee.as_base_units())
        .ok_or_else(|| {
            StateError::InvalidTx("payment channel create amount+fee overflow".into())
        })?;
    if owner.balance < debit {
        return Err(StateError::InvalidTx(
            "insufficient DRC balance for payment channel create".into(),
        ));
    }

    let mut live_ids = load_owner_index(store, &tx.owner)?;
    if live_ids.len() >= DRC_MAX_LIVE_PAYMENT_CHANNELS_PER_ACCOUNT {
        return Err(StateError::InvalidTx(
            "DRC live payment channel cap exceeded".into(),
        ));
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
            .ok_or_else(|| StateError::InvalidTx("payment channel create nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;

    live_ids.push(channel_id);
    live_ids.sort_by_key(|id| id.as_bytes().to_vec());
    put_owner_index(batch, &tx.owner, &live_ids)?;

    let live = DrcPaymentChannelLive {
        version: DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION,
        channel_id,
        owner: tx.owner,
        destination: tx.destination,
        destination_tag: tx.destination_tag,
        source_tag: tx.source_tag,
        invoice_id: tx.invoice_id,
        claim_public_key: tx.claim_public_key.clone(),
        settle_delay_blue_scores: tx.settle_delay_blue_scores,
        cancel_after_blue_score: tx.cancel_after_blue_score,
        total_funded: tx.amount,
        cumulative_claimed: Amount::from_base_units(0),
        close_finalizable_after: None,
        create_blue_score: application_blue_score,
    };
    put_live(batch, &live)?;
    Ok(channel_id)
}

pub fn apply_drc_payment_channel_fund(
    store: &StateStore,
    tx: &DrcPaymentChannelFundTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_payment_channel_fund_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.channel_id)?;
    let mut live = load_drc_payment_channel_live(store, &tx.channel_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown payment channel".into()))?;

    if !payment_channel_fund_submitter_allowed(tx.submitter, live.owner) {
        return Err(StateError::InvalidTx(
            "payment channel fund submitter must be owner".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &live.owner)?;
    let sequence_ctx = if tx.version >= DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if owner.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad payment channel fund nonce: got {} expected {}",
                tx.nonce, owner.nonce
            )));
        }
        None
    };

    let debit = tx
        .amount
        .as_base_units()
        .checked_add(tx.fee.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("payment channel fund amount+fee overflow".into()))?;
    if owner.balance < debit {
        return Err(StateError::InvalidTx(
            "insufficient DRC balance for payment channel fund".into(),
        ));
    }

    journal
        .before
        .push((NativeAssetId::DRC, live.owner, owner.clone()));
    owner.balance -= debit;
    if let Some(ctx) = sequence_ctx {
        finish_drc_account_sequence(
            batch,
            &tx.submitter,
            &mut owner,
            ctx.consumption,
            &ctx.tickets_before,
        )?;
    } else {
        owner.nonce = owner
            .nonce
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("payment channel fund nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &live.owner, &owner)?;

    live.total_funded = Amount::from_base_units(
        live.total_funded
            .as_base_units()
            .checked_add(tx.amount.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("payment channel total_funded overflow".into()))?,
    );
    put_live(batch, &live)?;

    let fund_event = DrcPaymentChannelFundEvent {
        version: DRC_PAYMENT_CHANNEL_FUND_EVENT_VERSION,
        channel_id: live.channel_id,
        fund_tx_id: tx.fund_tx_id(),
        amount: tx.amount,
        total_funded_after: live.total_funded,
        application_blue_score,
    };
    fund_event
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &payment_channel_fund_event_key(&tx.fund_tx_id()),
        &borsh::to_vec(&fund_event).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn apply_drc_payment_channel_claim(
    store: &StateStore,
    tx: &DrcPaymentChannelClaimTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_payment_channel_claim_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.channel_id)?;
    let mut live = load_drc_payment_channel_live(store, &tx.channel_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown payment channel".into()))?;

    if !payment_channel_claim_submitter_allowed(tx.submitter, live.destination) {
        return Err(StateError::InvalidTx(
            "payment channel claim submitter must be destination".into(),
        ));
    }
    if !payment_channel_onchain_claim_allowed(application_blue_score, live.cancel_after_blue_score)
    {
        return Err(StateError::InvalidTx(
            "payment channel on-chain claim not allowed at or after cancel after".into(),
        ));
    }

    let prev = live.cumulative_claimed.as_base_units();
    let new_cumulative = tx.cumulative_authorized.as_base_units();
    if new_cumulative <= prev {
        return Err(StateError::InvalidTx(
            "payment channel claim must increase cumulative authorized".into(),
        ));
    }
    if new_cumulative > live.total_funded.as_base_units() {
        return Err(StateError::InvalidTx(
            "payment channel claim exceeds total funded".into(),
        ));
    }
    let delta = new_cumulative - prev;

    let mut destination = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
    let sequence_ctx = if tx.version >= DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if destination.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad payment channel claim nonce: got {} expected {}",
                tx.nonce, destination.nonce
            )));
        }
        None
    };

    if destination.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC balance for payment channel claim fee".into(),
        ));
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &live.owner)?;
    if owner.balance < delta {
        return Err(StateError::InvalidTx(
            "insufficient owner balance for payment channel claim delta".into(),
        ));
    }

    journal
        .before
        .push((NativeAssetId::DRC, tx.submitter, destination.clone()));
    journal
        .before
        .push((NativeAssetId::DRC, live.owner, owner.clone()));

    destination.balance -= tx.fee.as_base_units();
    destination.balance = destination.balance.checked_add(delta).ok_or_else(|| {
        StateError::InvalidTx("payment channel claim destination overflow".into())
    })?;
    owner.balance -= delta;

    if let Some(ctx) = sequence_ctx {
        finish_drc_account_sequence(
            batch,
            &tx.submitter,
            &mut destination,
            ctx.consumption,
            &ctx.tickets_before,
        )?;
    } else {
        destination.nonce = destination
            .nonce
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("payment channel claim nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &tx.submitter, &destination)?;
    put_account_into(batch, NativeAssetId::DRC, &live.owner, &owner)?;

    live.cumulative_claimed = tx.cumulative_authorized;
    put_live(batch, &live)?;

    let claim_event = DrcPaymentChannelClaimEvent {
        version: DRC_PAYMENT_CHANNEL_CLAIM_EVENT_VERSION,
        channel_id: live.channel_id,
        claim_tx_id: tx.claim_tx_id(),
        cumulative_authorized: tx.cumulative_authorized,
        claim_delta: Amount::from_base_units(delta),
        application_blue_score,
    };
    claim_event
        .validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &payment_channel_claim_event_key(&tx.claim_tx_id()),
        &borsh::to_vec(&claim_event).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn apply_drc_payment_channel_close(
    store: &StateStore,
    tx: &DrcPaymentChannelCloseTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<Option<DrcPaymentChannelReceipt>, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_payment_channel_close_operation(store, tx, auth)?;

    ensure_unsettled(store, &tx.channel_id)?;
    let mut live = load_drc_payment_channel_live(store, &tx.channel_id)?
        .ok_or_else(|| StateError::InvalidTx("unknown payment channel".into()))?;

    if !payment_channel_close_submitter_allowed(
        tx.close_kind,
        tx.submitter,
        live.owner,
        live.destination,
    ) {
        return Err(StateError::InvalidTx(
            "payment channel close submitter not authorized for close kind".into(),
        ));
    }

    let mut submitter = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
    let sequence_ctx = if tx.version >= DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        if submitter.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad payment channel close nonce: got {} expected {}",
                tx.nonce, submitter.nonce
            )));
        }
        None
    };

    if submitter.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC balance for payment channel close fee".into(),
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
            .ok_or_else(|| StateError::InvalidTx("payment channel close nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, &tx.submitter, &submitter)?;

    match tx.close_kind {
        DrcPaymentChannelCloseKind::OwnerScheduleClose => {
            let finalize_at = payment_channel_owner_schedule_deadline(
                application_blue_score,
                live.settle_delay_blue_scores,
            )
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
            live.close_finalizable_after = Some(finalize_at);
            put_live(batch, &live)?;
            let schedule_event = DrcPaymentChannelScheduleEvent {
                version: DRC_PAYMENT_CHANNEL_SCHEDULE_EVENT_VERSION,
                channel_id: live.channel_id,
                close_tx_id: tx.close_tx_id(),
                close_finalizable_after: finalize_at,
                application_blue_score,
            };
            schedule_event
                .validate()
                .map_err(|e| StateError::Storage(e.to_string()))?;
            batch.put_cf(
                ColumnFamily::Meta,
                &payment_channel_schedule_event_key(&tx.close_tx_id()),
                &borsh::to_vec(&schedule_event).map_err(|e| StateError::Storage(e.to_string()))?,
            );
            Ok(None)
        }
        DrcPaymentChannelCloseKind::DestinationClose => Ok(Some(finalize_channel(
            store,
            batch,
            journal,
            &live,
            application_blue_score,
            tx.close_tx_id(),
        )?)),
        DrcPaymentChannelCloseKind::Finalize => {
            if !payment_channel_finalize_allowed(
                application_blue_score,
                live.close_finalizable_after,
                live.cancel_after_blue_score,
            ) {
                return Err(StateError::InvalidTx(
                    "payment channel not yet finalizable at this blue score".into(),
                ));
            }
            Ok(Some(finalize_channel(
                store,
                batch,
                journal,
                &live,
                application_blue_score,
                tx.close_tx_id(),
            )?))
        }
    }
}

pub fn lookup_drc_payment_channel_point(
    store: &StateStore,
    channel_id: &Hash,
) -> Result<&'static str, StateError> {
    if *channel_id == Hash::ZERO {
        return Err(StateError::InvalidTx("zero payment channel id".into()));
    }
    if load_drc_payment_channel_live(store, channel_id)?.is_some() {
        return Ok("live");
    }
    if load_drc_payment_channel_receipt(store, channel_id)?.is_some() {
        return Ok("receipt");
    }
    Ok("unknown")
}
