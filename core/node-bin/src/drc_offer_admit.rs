//! Public mempool admission for native DRC OfferCreate and OfferCancel.

use std::sync::Mutex;

use agora_p2p::Mempool;
use agora_rpc::RpcError;
use agora_state_machine::{
    apply_drc_offer_cancel, apply_drc_offer_create, load_account, load_drc_offer_cancel_receipt,
    load_drc_offer_create_receipt, load_drc_offer_live, load_drc_trust_line_live,
    load_issued_offer_reserve, AccountJournal, DrcOfferApplyLimits, StateStore, TxAuthContext,
    WriteBatch,
};
use agora_types::{DrcBookAsset, DrcOfferCancelTx, DrcOfferCreateTx, Hash, NativeAssetId};
use serde_json::{json, Value};

use crate::backend::min_relay_fee;

pub(crate) fn revalidate_drc_offer_mempool(
    store: &StateStore,
    pool: &mut Mempool,
    auth: &TxAuthContext,
    blue_score: u64,
) {
    let creates = pool.pending_offer_creates();
    for tx in creates {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        if apply_drc_offer_create(
            store,
            &tx,
            auth,
            blue_score,
            DrcOfferApplyLimits::unbounded(),
            &mut batch,
            &mut journal,
        )
        .is_err()
        {
            let _ = pool.remove_drc_offer_create(&tx.offer_id());
        }
    }
    let cancels = pool.pending_offer_cancels();
    for tx in cancels {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        if apply_drc_offer_cancel(store, &tx, auth, blue_score, &mut batch, &mut journal).is_err() {
            let _ = pool.remove_drc_offer_cancel(&tx.cancel_tx_id());
        }
    }
}

fn require_native_room(
    store: &StateStore,
    pool: &Mempool,
    owner: agora_types::Address,
    additional: u64,
) -> Result<(), RpcError> {
    let account = load_account(store, NativeAssetId::DRC, &owner)
        .map_err(|error| RpcError::Internal(error.to_string()))?;
    let pending = pool.pending_native_offer_lock(&owner);
    let needed = pending.saturating_add(additional);
    if account.balance < needed {
        return Err(RpcError::Rejected(
            "insufficient DRC for pending offer reservation".into(),
        ));
    }
    Ok(())
}

fn require_issued_room(
    store: &StateStore,
    pool: &Mempool,
    tx: &DrcOfferCreateTx,
) -> Result<(), RpcError> {
    let DrcBookAsset::Issued(asset) = tx.taker_gets else {
        return Ok(());
    };
    if asset.issuer == tx.owner {
        return Ok(());
    }
    let Some(line) = load_drc_trust_line_live(store, &tx.owner, &asset)
        .map_err(|error| RpcError::Internal(error.to_string()))?
    else {
        return Err(RpcError::Rejected("missing trust line".into()));
    };
    let reserved = load_issued_offer_reserve(store, &tx.owner, &asset)
        .map_err(|error| RpcError::Internal(error.to_string()))?;
    let pending = pool.pending_issued_offer_reserve(&tx.owner, &asset.asset_key());
    let needed = reserved
        .saturating_add(pending)
        .saturating_add(tx.taker_gets_amount);
    if line.balance.as_units() < needed {
        return Err(RpcError::Rejected(
            "issued balance is reserved by a DRC offer".into(),
        ));
    }
    Ok(())
}

pub(crate) fn admit_drc_offer_create(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcOfferCreateTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.owner)
        && tx.version < agora_types::DRC_OFFER_CREATE_TICKET_VERSION
    {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    let mut native = tx.fee.as_base_units();
    if tx.taker_gets.is_native() {
        native = native.saturating_add(tx.taker_gets_amount);
    }
    require_native_room(store, &pool, tx.owner, native)?;
    require_issued_room(store, &pool, &tx)?;
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_offer_create(
        store,
        &tx,
        auth,
        application_blue_score,
        DrcOfferApplyLimits::unbounded(),
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC offer create: {error}")))?;
    pool.admit_drc_offer_create(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn admit_drc_offer_cancel(
    store: &StateStore,
    mempool: &Mutex<Mempool>,
    tx: DrcOfferCancelTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
) -> Result<Hash, RpcError> {
    tx.validate_structure()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    if tx.fee.as_base_units() < min_relay_fee() {
        return Err(RpcError::Rejected(format!(
            "fee too low: {} < min relay {}",
            tx.fee.as_base_units(),
            min_relay_fee()
        )));
    }
    let mut pool = mempool
        .lock()
        .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?;
    if pool.account_reserved(NativeAssetId::DRC, &tx.submitter)
        && tx.version < agora_types::DRC_OFFER_CANCEL_TICKET_VERSION
    {
        return Err(RpcError::Rejected(
            "DRC account already has a pending nonce".into(),
        ));
    }
    if pool.offer_cancel_reserved(&tx.offer_id) {
        return Err(RpcError::Rejected(
            "DRC offer already has a pending cancel".into(),
        ));
    }
    require_native_room(store, &pool, tx.submitter, tx.fee.as_base_units())?;
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_offer_cancel(
        store,
        &tx,
        auth,
        application_blue_score,
        &mut batch,
        &mut journal,
    )
    .map_err(|error| RpcError::Rejected(format!("DRC offer cancel: {error}")))?;
    pool.admit_drc_offer_cancel(tx)
        .map_err(|error| RpcError::Rejected(error.to_string()))
}

pub(crate) fn drc_offer_json(
    store: &StateStore,
    offer_id: &Hash,
    blue_score: u64,
) -> Result<Value, RpcError> {
    match load_drc_offer_live(store, offer_id)
        .map_err(|error| RpcError::Internal(error.to_string()))?
    {
        Some(offer) => {
            let funded = agora_state_machine::list_account_offers(
                store,
                &offer.owner,
                None,
                agora_types::DRC_MAX_LIVE_OFFERS_PER_ACCOUNT,
                blue_score,
            )
            .map_err(|error| RpcError::Internal(error.to_string()))?
            .offers
            .into_iter()
            .find(|view| view.offer.offer_id == offer.offer_id)
            .map(|view| view.taker_gets_funded)
            .unwrap_or(0);
            Ok(json!({
                "status": "live",
                "offer": offer,
                "taker_gets_funded": funded,
                "expired": offer.expired_at(blue_score),
                "simulated_fill": false,
            }))
        }
        None => {
            let receipt = drc_offer_create_receipt_json(store, offer_id)?;
            Ok(json!({
                "offer_id": offer_id.to_hex(),
                "status": receipt.get("status").cloned().unwrap_or(json!("unknown")),
                "receipt": receipt.get("receipt").cloned(),
                "simulated_fill": false,
            }))
        }
    }
}

pub(crate) fn drc_offer_create_receipt_json(
    store: &StateStore,
    offer_id: &Hash,
) -> Result<Value, RpcError> {
    match load_drc_offer_create_receipt(store, offer_id)
        .map_err(|error| RpcError::Internal(error.to_string()))?
    {
        Some(receipt) => Ok(json!({
            "status": "known",
            "receipt": receipt,
            "simulated_fill": false,
        })),
        None => Ok(json!({
            "offer_id": offer_id.to_hex(),
            "status": "unknown",
        })),
    }
}

#[allow(dead_code)]
pub(crate) fn drc_offer_cancel_receipt_json(
    store: &StateStore,
    cancel_tx_id: &Hash,
) -> Result<Value, RpcError> {
    match load_drc_offer_cancel_receipt(store, cancel_tx_id)
        .map_err(|error| RpcError::Internal(error.to_string()))?
    {
        Some(receipt) => Ok(json!({
            "status": "known",
            "receipt": receipt,
        })),
        None => Ok(json!({
            "cancel_tx_id": cancel_tx_id.to_hex(),
            "status": "unknown",
        })),
    }
}
