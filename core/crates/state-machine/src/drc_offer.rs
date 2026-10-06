//! Native DRC order book. Crossing uses integer rates and a committed book sequence.
//!
//! Funds sold by a resting offer are reserved at placement so later payments,
//! checks, escrow, channels, and other offers cannot spend them. Issuer accounts
//! may sell their own issued asset without a holder balance. Native DRC is never
//! an issued asset and never inherits issuer freeze or authorization.

use std::collections::{BTreeSet, HashMap};

use agora_types::{
    resolve_drc_account_sequence, Address, DrcBookAsset, DrcOfferBook, DrcOfferBookCursor,
    DrcOfferBookPage, DrcOfferCancelOutcome, DrcOfferCancelReceipt, DrcOfferCancelTx,
    DrcOfferCreateReceipt, DrcOfferCreateTx, DrcOfferCursor, DrcOfferFillMode, DrcOfferLive,
    DrcOfferPage, DrcOfferTimeInForce, DrcOfferView, DrcTrustLineLive, Hash, IssuedAmount,
    IssuedAssetId, NativeAssetId, DRC_MAX_LIVE_OFFERS_PER_ACCOUNT, DRC_MAX_OFFERS_PER_BOOK,
    DRC_MAX_OFFER_MATCHES_PER_TX, DRC_OFFER_CANCEL_RECEIPT_VERSION,
    DRC_OFFER_CANCEL_TICKET_VERSION, DRC_OFFER_CREATE_RECEIPT_VERSION,
    DRC_OFFER_CREATE_TICKET_VERSION, DRC_OFFER_LIVE_STATE_VERSION, DRC_OFFER_PAGE_MAX,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::accounts::{load_account, put_account_into, AccountJournal, AccountState};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::{
    verify_drc_offer_cancel_operation, verify_drc_offer_create_operation,
};
use crate::drc_issued_controls::{
    issued_movement_allowed, load_drc_issued_asset_policy, normalize_trust_line_live,
    IssuedMovementKind,
};
use crate::drc_ticket::{begin_drc_account_sequence, finish_drc_account_sequence};
use crate::drc_trust_line::{
    issuer_liability_key, load_drc_issuer_liability, load_drc_trust_line_live, put_liability,
    put_live, trust_line_key,
};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const LIVE_PREFIX: &[u8] = b"offer/drc/live/";
const OWNER_INDEX_PREFIX: &[u8] = b"offer/drc/owner/";
const BOOK_INDEX_PREFIX: &[u8] = b"offer/drc/book/";
const SEQUENCE_KEY: &[u8] = b"offer/drc/sequence";
const RESERVE_PREFIX: &[u8] = b"offer/drc/reserve/";
const CREATE_RECEIPT_PREFIX: &[u8] = b"offer/drc/create-receipt/";
const CANCEL_RECEIPT_PREFIX: &[u8] = b"offer/drc/cancel-receipt/";
pub const DRC_OFFER_ROOT_DOMAIN: &[u8] = b"agora-drc-offer-root-v1";

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
struct OwnerIndex {
    version: u32,
    owner: Address,
    live_ids: Vec<Hash>,
}

#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
struct BookIndex {
    version: u32,
    book_key: Hash,
    live_ids: Vec<Hash>,
}

const INDEX_VERSION: u32 = 1;

pub struct DrcOfferApplyLimits {
    pub block_steps_remaining: usize,
}

impl DrcOfferApplyLimits {
    pub fn unbounded() -> Self {
        Self {
            block_steps_remaining: DRC_MAX_OFFER_MATCHES_PER_TX,
        }
    }
}

pub struct DrcOfferCreateApplied {
    pub receipt: DrcOfferCreateReceipt,
    pub meta_keys: Vec<Vec<u8>>,
    pub steps: usize,
}

pub struct DrcOfferCancelApplied {
    pub receipt: DrcOfferCancelReceipt,
    pub meta_keys: Vec<Vec<u8>>,
}

pub fn offer_live_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(LIVE_PREFIX.len() + 32);
    key.extend_from_slice(LIVE_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn offer_owner_index_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(OWNER_INDEX_PREFIX.len() + 20);
    key.extend_from_slice(OWNER_INDEX_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn offer_book_index_key(book: DrcOfferBook) -> Vec<u8> {
    let mut key = Vec::with_capacity(BOOK_INDEX_PREFIX.len() + 32);
    key.extend_from_slice(BOOK_INDEX_PREFIX);
    key.extend_from_slice(book.book_key().as_bytes());
    key
}

fn reserve_key(holder: &Address, asset: &IssuedAssetId) -> Vec<u8> {
    let mut key = Vec::with_capacity(RESERVE_PREFIX.len() + 20 + 32);
    key.extend_from_slice(RESERVE_PREFIX);
    key.extend_from_slice(&holder.0);
    key.extend_from_slice(asset.asset_key().as_bytes());
    key
}

fn create_receipt_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(CREATE_RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(CREATE_RECEIPT_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

fn cancel_receipt_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(CANCEL_RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(CANCEL_RECEIPT_PREFIX);
    key.extend_from_slice(id.as_bytes());
    key
}

pub fn load_drc_offer_live(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<DrcOfferLive>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &offer_live_key(id))? else {
        return Ok(None);
    };
    decode_live(id, &bytes).map(Some)
}

pub fn load_drc_offer_create_receipt(
    store: &StateStore,
    offer_id: &Hash,
) -> Result<Option<DrcOfferCreateReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &create_receipt_key(offer_id))? else {
        return Ok(None);
    };
    DrcOfferCreateReceipt::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_drc_offer_cancel_receipt(
    store: &StateStore,
    cancel_tx_id: &Hash,
) -> Result<Option<DrcOfferCancelReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &cancel_receipt_key(cancel_tx_id))? else {
        return Ok(None);
    };
    DrcOfferCancelReceipt::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn load_issued_offer_reserve(
    store: &StateStore,
    holder: &Address,
    asset: &IssuedAssetId,
) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &reserve_key(holder, asset))? else {
        return Ok(0);
    };
    if bytes.len() != 8 {
        return Err(StateError::Storage("invalid DRC offer reserve".into()));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(buf))
}

/// Reject spends that would consume units already reserved by live offers.
pub fn assert_issued_offer_reserve(
    store: &StateStore,
    holder: &Address,
    asset: &IssuedAssetId,
    spend: u64,
) -> Result<(), StateError> {
    let Some(line) = load_drc_trust_line_live(store, holder, asset)? else {
        return Err(StateError::InvalidTx("missing trust line".into()));
    };
    let reserved = load_issued_offer_reserve(store, holder, asset)?;
    let available = line.balance.as_units().saturating_sub(reserved);
    if available < spend {
        return Err(StateError::InvalidTx(
            "issued balance is reserved by a DRC offer".into(),
        ));
    }
    Ok(())
}

pub fn drc_offer_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, LIVE_PREFIX)? {
        if key.len() != LIVE_PREFIX.len() + 32 {
            return Err(StateError::Storage("invalid DRC offer live key".into()));
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(&key[LIVE_PREFIX.len()..]);
        entries.push(decode_live(&Hash(id), &bytes)?);
    }
    entries.sort_by_key(|offer| offer.offer_id.as_bytes().to_vec());
    Ok(Hash::hash_borsh(&(DRC_OFFER_ROOT_DOMAIN, entries)))
}

pub fn list_account_offers(
    store: &StateStore,
    owner: &Address,
    cursor: Option<DrcOfferCursor>,
    limit: usize,
    blue_score: u64,
) -> Result<DrcOfferPage, StateError> {
    let limit = limit.clamp(1, DRC_OFFER_PAGE_MAX);
    let mut ids = load_owner_ids(store, owner)?;
    ids.sort_by_key(|id| id.as_bytes().to_vec());
    if let Some(cursor) = cursor {
        let Some(pos) = ids.iter().position(|id| *id == cursor.offer_id) else {
            return Err(StateError::InvalidTx("stale DRC offer cursor".into()));
        };
        ids = ids.split_off(pos + 1);
    }
    let mut offers = Vec::new();
    for id in &ids {
        if offers.len() == limit {
            break;
        }
        if let Some(offer) = load_drc_offer_live(store, id)? {
            offers.push(view_offer(store, offer, blue_score)?);
        }
    }
    let next_cursor = if ids.len() > offers.len() {
        offers
            .last()
            .map(|view| DrcOfferCursor::from_offer_id(view.offer.offer_id))
    } else {
        None
    };
    Ok(DrcOfferPage {
        version: 1,
        offers,
        next_cursor,
    })
}

pub fn list_book_offers(
    store: &StateStore,
    book: DrcOfferBook,
    cursor: Option<DrcOfferBookCursor>,
    limit: usize,
    blue_score: u64,
) -> Result<DrcOfferBookPage, StateError> {
    book.validate()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    let limit = limit.clamp(1, DRC_OFFER_PAGE_MAX);
    let mut offers = load_book(store, book)?;
    sort_book(&mut offers);
    if let Some(cursor) = cursor {
        if let Some(pos) = offers
            .iter()
            .position(|offer| DrcOfferBookCursor::from_live(offer) == cursor)
        {
            offers.drain(..=pos);
        } else {
            offers.retain(|offer| book_sorts_after(offer, &cursor));
        }
    }
    let more = offers.len() > limit;
    offers.truncate(limit);
    let next_cursor = if more {
        offers.last().map(DrcOfferBookCursor::from_live)
    } else {
        None
    };
    let mut views = Vec::with_capacity(offers.len());
    for offer in offers {
        views.push(view_offer(store, offer, blue_score)?);
    }
    Ok(DrcOfferBookPage {
        version: 1,
        book,
        offers: views,
        next_cursor,
    })
}

fn book_sorts_after(offer: &DrcOfferLive, cursor: &DrcOfferBookCursor) -> bool {
    let offer_better = agora_types::offer_quality_better(
        offer.taker_gets_original,
        offer.taker_pays_original,
        cursor.gets_original,
        cursor.pays_original,
    );
    if offer_better {
        return false;
    }
    let cursor_better = agora_types::offer_quality_better(
        cursor.gets_original,
        cursor.pays_original,
        offer.taker_gets_original,
        offer.taker_pays_original,
    );
    if cursor_better {
        return true;
    }
    (offer.book_sequence, offer.offer_id.as_bytes())
        > (cursor.book_sequence, cursor.offer_id.as_bytes())
}

pub fn apply_drc_offer_create(
    store: &StateStore,
    tx: &DrcOfferCreateTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    limits: DrcOfferApplyLimits,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcOfferCreateApplied, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_offer_create_operation(store, tx, auth)?;
    let offer_id = tx.offer_id();
    if load_drc_offer_live(store, &offer_id)?.is_some() {
        return Err(StateError::InvalidTx("duplicate DRC offer id".into()));
    }
    if tx
        .expires_after_blue_score
        .is_some_and(|bound| application_blue_score > bound)
    {
        return Err(StateError::InvalidTx(
            "DRC offer expiration is already past".into(),
        ));
    }

    let sequence_ctx = if tx.version >= DRC_OFFER_CREATE_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_OFFER_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.owner, selector)?)
    } else {
        let owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
        if owner.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC offer-create nonce: got {} expected {}",
                tx.nonce, owner.nonce
            )));
        }
        None
    };

    let mut work = Working::new(store)?;
    work.account(&tx.owner)?;
    work.debit_native(&tx.owner, tx.fee.as_base_units())?;
    if !work.seller_can_fund(&tx.owner, tx.taker_gets, 1)? {
        return Err(StateError::InvalidTx(
            "DRC offer owner is unfunded for the sell asset".into(),
        ));
    }

    let step_cap = limits
        .block_steps_remaining
        .min(DRC_MAX_OFFER_MATCHES_PER_TX);
    let mut steps = 0usize;
    let mut self_cross_cancelled = 0u32;
    let mut removed = Vec::new();
    let mut pays_remaining = tx.taker_pays_amount;
    let mut gets_remaining = tx.taker_gets_amount;
    let mut taker_paid = 0u64;
    let mut taker_received = 0u64;

    let own = work.owner_ids(&tx.owner)?;
    for id in own {
        let Some(existing) = work.offer(&id)?.cloned() else {
            continue;
        };
        if existing.book() != tx.book().opposite() {
            continue;
        }
        if !agora_types::offers_cross(
            gets_remaining,
            pays_remaining,
            existing.taker_gets_remaining,
            existing.taker_pays_remaining,
        ) {
            continue;
        }
        steps = bump_steps(steps, step_cap)?;
        work.release_offer(&existing)?;
        self_cross_cancelled += 1;
        removed.push(id);
    }

    let mut makers = work.book_offers(tx.book().opposite())?;
    sort_book(&mut makers);
    let fill_mode = DrcOfferFillMode::from_u8(tx.fill_mode)
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    for maker in makers {
        if removed.contains(&maker.offer_id) {
            continue;
        }
        if taker_done(fill_mode, pays_remaining, gets_remaining) {
            break;
        }
        let Some(maker) = work.offer(&maker.offer_id)?.cloned() else {
            continue;
        };
        if maker.expired_at(application_blue_score) {
            steps = bump_steps(steps, step_cap)?;
            work.release_offer(&maker)?;
            removed.push(maker.offer_id);
            continue;
        }
        match work.cross_one(
            &tx.owner,
            tx.taker_gets,
            tx.taker_pays,
            &maker,
            gets_remaining,
            pays_remaining,
        )? {
            Cross::Skip => continue,
            Cross::Stop => break,
            Cross::Remove => {
                steps = bump_steps(steps, step_cap)?;
                work.release_offer(&maker)?;
                removed.push(maker.offer_id);
            }
            Cross::Fill { paid, received } => {
                steps = bump_steps(steps, step_cap)?;
                gets_remaining -= paid;
                pays_remaining -= received;
                taker_paid += paid;
                taker_received += received;
                let Some(mut updated) = work.offer(&maker.offer_id)?.cloned() else {
                    continue;
                };
                if updated.taker_gets_remaining == 0 || updated.taker_pays_remaining == 0 {
                    work.release_offer(&updated)?;
                    removed.push(updated.offer_id);
                } else {
                    work.persist_offer(&updated)?;
                }
                let _ = &mut updated;
            }
        }
    }

    let fully_filled = taker_done(fill_mode, pays_remaining, gets_remaining);
    let time_in_force = DrcOfferTimeInForce::from_u8(tx.time_in_force)
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    if time_in_force == DrcOfferTimeInForce::FillOrKill && !fully_filled {
        return Err(StateError::InvalidTx(
            "DRC offer fill-or-kill was not fully filled".into(),
        ));
    }

    let mut placed = false;
    if time_in_force == DrcOfferTimeInForce::GoodTillCancel
        && !fully_filled
        && pays_remaining > 0
        && gets_remaining > 0
    {
        if work.seller_can_fund(&tx.owner, tx.taker_gets, gets_remaining)? {
            let sequence = work.alloc_sequence()?;
            let live = DrcOfferLive {
                version: DRC_OFFER_LIVE_STATE_VERSION,
                offer_id,
                owner: tx.owner,
                taker_pays: tx.taker_pays,
                taker_pays_original: tx.taker_pays_amount,
                taker_pays_remaining: pays_remaining,
                taker_gets: tx.taker_gets,
                taker_gets_original: tx.taker_gets_amount,
                taker_gets_remaining: gets_remaining,
                fill_mode: tx.fill_mode,
                book_sequence: sequence,
                create_blue_score: application_blue_score,
                expires_after_blue_score: tx.expires_after_blue_score,
                native_locked: 0,
            };
            work.reserve_new(&live)?;
            placed = true;
        } else if taker_paid == 0 {
            return Err(StateError::InvalidTx(
                "DRC offer owner is unfunded for the sell asset".into(),
            ));
        }
    } else if taker_paid == 0 && time_in_force == DrcOfferTimeInForce::GoodTillCancel && !placed {
        return Err(StateError::InvalidTx(
            "DRC offer owner is unfunded for the sell asset".into(),
        ));
    }

    removed.sort_by_key(|id| id.as_bytes().to_vec());
    removed.dedup();
    let receipt = DrcOfferCreateReceipt {
        version: DRC_OFFER_CREATE_RECEIPT_VERSION,
        offer_id,
        owner: tx.owner,
        placed,
        fully_filled,
        taker_paid,
        taker_received,
        steps: u32::try_from(steps).unwrap_or(u32::MAX),
        self_cross_cancelled,
        removed_offer_ids: removed,
        fee: tx.fee,
        settlement_blue_score: application_blue_score,
    };
    let meta_keys = work.flush(
        batch,
        journal,
        &tx.owner,
        sequence_ctx,
        tx.version >= DRC_OFFER_CREATE_TICKET_VERSION,
    )?;
    batch.put_cf(
        ColumnFamily::Meta,
        &create_receipt_key(&offer_id),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    let mut meta_keys = meta_keys;
    meta_keys.push(create_receipt_key(&offer_id));
    Ok(DrcOfferCreateApplied {
        receipt,
        meta_keys,
        steps,
    })
}

pub fn apply_drc_offer_cancel(
    store: &StateStore,
    tx: &DrcOfferCancelTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcOfferCancelApplied, StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_offer_cancel_operation(store, tx, auth)?;
    let sequence_ctx = if tx.version >= DRC_OFFER_CANCEL_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_OFFER_CANCEL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.submitter, selector)?)
    } else {
        let submitter = load_account(store, NativeAssetId::DRC, &tx.submitter)?;
        if submitter.nonce != tx.nonce {
            return Err(StateError::InvalidTx(format!(
                "bad DRC offer-cancel nonce: got {} expected {}",
                tx.nonce, submitter.nonce
            )));
        }
        None
    };
    let mut work = Working::new(store)?;
    work.account(&tx.submitter)?;
    work.debit_native(&tx.submitter, tx.fee.as_base_units())?;
    let outcome = if let Some(live) = work.offer(&tx.offer_id)?.cloned() {
        if live.owner != tx.submitter {
            return Err(StateError::InvalidTx(
                "DRC offer cancel submitter is not the owner".into(),
            ));
        }
        work.release_offer(&live)?;
        DrcOfferCancelOutcome::Removed
    } else {
        DrcOfferCancelOutcome::Absent
    };
    let receipt = DrcOfferCancelReceipt {
        version: DRC_OFFER_CANCEL_RECEIPT_VERSION,
        cancel_tx_id: tx.cancel_tx_id(),
        offer_id: tx.offer_id,
        submitter: tx.submitter,
        outcome,
        fee: tx.fee,
        settlement_blue_score: application_blue_score,
    };
    let meta_keys = work.flush(
        batch,
        journal,
        &tx.submitter,
        sequence_ctx,
        tx.version >= DRC_OFFER_CANCEL_TICKET_VERSION,
    )?;
    batch.put_cf(
        ColumnFamily::Meta,
        &cancel_receipt_key(&tx.cancel_tx_id()),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    let mut meta_keys = meta_keys;
    meta_keys.push(cancel_receipt_key(&tx.cancel_tx_id()));
    Ok(DrcOfferCancelApplied { receipt, meta_keys })
}

fn bump_steps(steps: usize, cap: usize) -> Result<usize, StateError> {
    let next = steps + 1;
    if next > cap {
        return Err(StateError::InvalidTx("DRC offer match cap exceeded".into()));
    }
    Ok(next)
}

fn taker_done(mode: DrcOfferFillMode, pays_remaining: u64, gets_remaining: u64) -> bool {
    match mode {
        DrcOfferFillMode::Buy => pays_remaining == 0,
        DrcOfferFillMode::Sell => gets_remaining == 0,
    }
}

fn sort_book(offers: &mut [DrcOfferLive]) {
    offers.sort_by(|left, right| {
        if agora_types::offer_quality_better(
            left.taker_gets_original,
            left.taker_pays_original,
            right.taker_gets_original,
            right.taker_pays_original,
        ) {
            std::cmp::Ordering::Less
        } else if agora_types::offer_quality_better(
            right.taker_gets_original,
            right.taker_pays_original,
            left.taker_gets_original,
            left.taker_pays_original,
        ) {
            std::cmp::Ordering::Greater
        } else {
            left.book_sequence
                .cmp(&right.book_sequence)
                .then_with(|| left.offer_id.as_bytes().cmp(right.offer_id.as_bytes()))
        }
    });
}

fn decode_live(id: &Hash, bytes: &[u8]) -> Result<DrcOfferLive, StateError> {
    let live =
        DrcOfferLive::try_from_slice(bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if live.offer_id != *id {
        return Err(StateError::Storage(
            "DRC offer live id does not match index key".into(),
        ));
    }
    Ok(live)
}

fn load_owner_ids(store: &StateStore, owner: &Address) -> Result<Vec<Hash>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &offer_owner_index_key(owner))? else {
        return Ok(Vec::new());
    };
    let record =
        OwnerIndex::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if record.version != INDEX_VERSION || record.owner != *owner {
        return Err(StateError::Storage("invalid DRC offer owner index".into()));
    }
    Ok(record.live_ids)
}

fn load_book(store: &StateStore, book: DrcOfferBook) -> Result<Vec<DrcOfferLive>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &offer_book_index_key(book))? else {
        return Ok(Vec::new());
    };
    let record =
        BookIndex::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if record.version != INDEX_VERSION || record.book_key != book.book_key() {
        return Err(StateError::Storage("invalid DRC offer book index".into()));
    }
    let mut offers = Vec::new();
    for id in record.live_ids {
        if let Some(live) = load_drc_offer_live(store, &id)? {
            offers.push(live);
        }
    }
    Ok(offers)
}

fn view_offer(
    store: &StateStore,
    offer: DrcOfferLive,
    blue_score: u64,
) -> Result<DrcOfferView, StateError> {
    let funded = funded_gets(store, &offer)?;
    let expired = offer.expired_at(blue_score);
    Ok(DrcOfferView {
        offer,
        taker_gets_funded: funded,
        expired,
    })
}

fn funded_gets(store: &StateStore, offer: &DrcOfferLive) -> Result<u64, StateError> {
    if offer.taker_gets.is_native() {
        return Ok(offer.native_locked.min(offer.taker_gets_remaining));
    }
    let Some(asset) = offer.taker_gets.issued() else {
        return Ok(0);
    };
    if offer.owner == asset.issuer {
        return Ok(offer.taker_gets_remaining);
    }
    let Some(line) = load_drc_trust_line_live(store, &offer.owner, &asset)? else {
        return Ok(0);
    };
    let policy = load_drc_issued_asset_policy(store, &asset)?;
    let line = normalize_trust_line_live(line, &policy);
    if !line.authorized || line.line_deep_frozen {
        return Ok(0);
    }
    Ok(line.balance.as_units().min(offer.taker_gets_remaining))
}

enum Cross {
    Fill { paid: u64, received: u64 },
    Remove,
    Skip,
    Stop,
}

struct Working<'a> {
    store: &'a StateStore,
    accounts: HashMap<Address, AccountState>,
    originals: HashMap<Address, AccountState>,
    lines: HashMap<(Address, IssuedAssetId), DrcTrustLineLive>,
    line_dirty: BTreeSet<(Address, IssuedAssetId)>,
    liability: HashMap<IssuedAssetId, IssuedAmount>,
    liability_dirty: BTreeSet<IssuedAssetId>,
    reserve: HashMap<(Address, IssuedAssetId), u64>,
    reserve_dirty: BTreeSet<(Address, IssuedAssetId)>,
    offers: HashMap<Hash, DrcOfferLive>,
    offer_deleted: BTreeSet<Hash>,
    owners: HashMap<Address, Vec<Hash>>,
    owner_dirty: BTreeSet<Address>,
    books: HashMap<Hash, (DrcOfferBook, Vec<Hash>)>,
    book_dirty: BTreeSet<Hash>,
    sequence: u64,
    sequence_dirty: bool,
}

impl<'a> Working<'a> {
    fn new(store: &'a StateStore) -> Result<Self, StateError> {
        let sequence = store
            .get_cf(ColumnFamily::Meta, SEQUENCE_KEY)?
            .map(|bytes| {
                if bytes.len() != 8 {
                    return Err(StateError::Storage("invalid DRC offer sequence".into()));
                }
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&bytes);
                Ok(u64::from_le_bytes(buf))
            })
            .transpose()?
            .unwrap_or(0);
        Ok(Self {
            store,
            accounts: HashMap::new(),
            originals: HashMap::new(),
            lines: HashMap::new(),
            line_dirty: BTreeSet::new(),
            liability: HashMap::new(),
            liability_dirty: BTreeSet::new(),
            reserve: HashMap::new(),
            reserve_dirty: BTreeSet::new(),
            offers: HashMap::new(),
            offer_deleted: BTreeSet::new(),
            owners: HashMap::new(),
            owner_dirty: BTreeSet::new(),
            books: HashMap::new(),
            book_dirty: BTreeSet::new(),
            sequence,
            sequence_dirty: false,
        })
    }

    fn account(&mut self, owner: &Address) -> Result<&mut AccountState, StateError> {
        if !self.accounts.contains_key(owner) {
            let loaded = load_account(self.store, NativeAssetId::DRC, owner)?;
            self.originals.insert(*owner, loaded.clone());
            self.accounts.insert(*owner, loaded);
        }
        Ok(self.accounts.get_mut(owner).expect("account inserted"))
    }

    fn debit_native(&mut self, owner: &Address, amount: u64) -> Result<(), StateError> {
        if amount == 0 {
            return Ok(());
        }
        let account = self.account(owner)?;
        if account.balance < amount {
            return Err(StateError::InvalidTx(
                "insufficient DRC offer balance".into(),
            ));
        }
        account.balance -= amount;
        Ok(())
    }

    fn credit_native(&mut self, owner: &Address, amount: u64) -> Result<(), StateError> {
        if amount == 0 {
            return Ok(());
        }
        let account = self.account(owner)?;
        account.balance = account
            .balance
            .checked_add(amount)
            .ok_or_else(|| StateError::InvalidTx("DRC offer balance overflow".into()))?;
        Ok(())
    }

    fn offer(&mut self, id: &Hash) -> Result<Option<&DrcOfferLive>, StateError> {
        if self.offer_deleted.contains(id) {
            return Ok(None);
        }
        if !self.offers.contains_key(id) {
            if let Some(live) = load_drc_offer_live(self.store, id)? {
                self.offers.insert(*id, live);
            } else {
                return Ok(None);
            }
        }
        Ok(self.offers.get(id))
    }

    fn owner_ids(&mut self, owner: &Address) -> Result<Vec<Hash>, StateError> {
        if !self.owners.contains_key(owner) {
            self.owners
                .insert(*owner, load_owner_ids(self.store, owner)?);
        }
        Ok(self.owners.get(owner).cloned().unwrap_or_default())
    }

    fn book_offers(&mut self, book: DrcOfferBook) -> Result<Vec<DrcOfferLive>, StateError> {
        let key = book.book_key();
        if !self.books.contains_key(&key) {
            let loaded = load_book(self.store, book)?;
            let ids = loaded.iter().map(|offer| offer.offer_id).collect();
            for offer in &loaded {
                self.offers
                    .entry(offer.offer_id)
                    .or_insert_with(|| offer.clone());
            }
            self.books.insert(key, (book, ids));
        }
        let ids = self
            .books
            .get(&key)
            .map(|(_, ids)| ids.clone())
            .unwrap_or_default();
        let mut offers = Vec::new();
        for id in ids {
            if let Some(offer) = self.offer(&id)?.cloned() {
                offers.push(offer);
            }
        }
        Ok(offers)
    }

    fn seller_can_fund(
        &mut self,
        owner: &Address,
        asset: DrcBookAsset,
        amount: u64,
    ) -> Result<bool, StateError> {
        if amount == 0 {
            return Ok(false);
        }
        if asset.is_native() {
            return Ok(self.account(owner)?.balance >= amount);
        }
        let Some(issued) = asset.issued() else {
            return Ok(false);
        };
        if *owner == issued.issuer {
            return Ok(true);
        }
        let Some(line) = self.line(owner, &issued)? else {
            return Ok(false);
        };
        let policy = load_drc_issued_asset_policy(self.store, &issued)?;
        let line = normalize_trust_line_live(line, &policy);
        if !line.authorized || line.line_deep_frozen {
            return Ok(false);
        }
        let reserved = self.reserve_of(owner, &issued)?;
        Ok(line.balance.as_units().saturating_sub(reserved) >= amount)
    }

    fn line(
        &mut self,
        holder: &Address,
        asset: &IssuedAssetId,
    ) -> Result<Option<DrcTrustLineLive>, StateError> {
        let key = (*holder, *asset);
        if !self.lines.contains_key(&key) {
            if let Some(live) = load_drc_trust_line_live(self.store, holder, asset)? {
                self.lines.insert(key, live);
            } else {
                return Ok(None);
            }
        }
        Ok(self.lines.get(&key).cloned())
    }

    fn reserve_of(&mut self, holder: &Address, asset: &IssuedAssetId) -> Result<u64, StateError> {
        let key = (*holder, *asset);
        if !self.reserve.contains_key(&key) {
            let value = load_issued_offer_reserve(self.store, holder, asset)?;
            self.reserve.insert(key, value);
        }
        Ok(self.reserve.get(&key).copied().unwrap_or(0))
    }

    fn add_reserve(
        &mut self,
        holder: &Address,
        asset: &IssuedAssetId,
        amount: u64,
    ) -> Result<(), StateError> {
        if amount == 0 {
            return Ok(());
        }
        let current = self.reserve_of(holder, asset)?;
        let next = current
            .checked_add(amount)
            .ok_or_else(|| StateError::InvalidTx("DRC offer reserve overflow".into()))?;
        self.reserve.insert((*holder, *asset), next);
        self.reserve_dirty.insert((*holder, *asset));
        Ok(())
    }

    fn sub_reserve(
        &mut self,
        holder: &Address,
        asset: &IssuedAssetId,
        amount: u64,
    ) -> Result<(), StateError> {
        if amount == 0 {
            return Ok(());
        }
        let current = self.reserve_of(holder, asset)?;
        if current < amount {
            return Err(StateError::InvalidTx("DRC offer reserve underflow".into()));
        }
        self.reserve.insert((*holder, *asset), current - amount);
        self.reserve_dirty.insert((*holder, *asset));
        Ok(())
    }

    fn alloc_sequence(&mut self) -> Result<u64, StateError> {
        let next = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("DRC offer sequence overflow".into()))?;
        self.sequence = next;
        self.sequence_dirty = true;
        Ok(next)
    }

    fn reserve_new(&mut self, live: &DrcOfferLive) -> Result<(), StateError> {
        let mut stored = live.clone();
        if stored.taker_gets.is_native() {
            self.debit_native(&stored.owner, stored.taker_gets_remaining)?;
            stored.native_locked = stored.taker_gets_remaining;
        } else if let Some(asset) = stored.taker_gets.issued() {
            if stored.owner != asset.issuer {
                let line = self.line(&stored.owner, &asset)?.ok_or_else(|| {
                    StateError::InvalidTx("DRC offer owner is unfunded for the sell asset".into())
                })?;
                let reserved =
                    self.reserve_of(&stored.owner, &asset)? + stored.taker_gets_remaining;
                if line.balance.as_units() < reserved {
                    return Err(StateError::InvalidTx(
                        "DRC offer owner is unfunded for the sell asset".into(),
                    ));
                }
                self.add_reserve(&stored.owner, &asset, stored.taker_gets_remaining)?;
            }
        }
        self.insert_offer(stored)
    }

    fn insert_offer(&mut self, live: DrcOfferLive) -> Result<(), StateError> {
        live.validate()
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        let owner_ids = self.owner_ids(&live.owner)?;
        if owner_ids.len() >= DRC_MAX_LIVE_OFFERS_PER_ACCOUNT {
            return Err(StateError::InvalidTx("DRC live offer cap exceeded".into()));
        }
        let book = live.book();
        let book_ids = if let Some((_, ids)) = self.books.get(&book.book_key()) {
            ids.len()
        } else {
            load_book(self.store, book)?.len()
        };
        if book_ids >= DRC_MAX_OFFERS_PER_BOOK {
            return Err(StateError::InvalidTx("DRC offer book cap exceeded".into()));
        }
        let id = live.offer_id;
        let owner = live.owner;
        self.offers.insert(id, live);
        self.offer_deleted.remove(&id);
        let ids = self.owners.entry(owner).or_default();
        if !ids.contains(&id) {
            ids.push(id);
        }
        self.owner_dirty.insert(owner);
        if !self.books.contains_key(&book.book_key()) {
            let ids = load_book(self.store, book)?
                .into_iter()
                .map(|offer| offer.offer_id)
                .collect();
            self.books.insert(book.book_key(), (book, ids));
        }
        let entry = self
            .books
            .get_mut(&book.book_key())
            .expect("offer book loaded");
        if !entry.1.contains(&id) {
            entry.1.push(id);
        }
        self.book_dirty.insert(book.book_key());
        Ok(())
    }

    fn persist_offer(&mut self, live: &DrcOfferLive) -> Result<(), StateError> {
        live.validate()
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        self.offers.insert(live.offer_id, live.clone());
        Ok(())
    }

    fn release_offer(&mut self, live: &DrcOfferLive) -> Result<(), StateError> {
        if live.native_locked > 0 {
            self.credit_native(&live.owner, live.native_locked)?;
        }
        if let Some(asset) = live.taker_gets.issued() {
            if live.owner != asset.issuer {
                self.sub_reserve(&live.owner, &asset, live.taker_gets_remaining)?;
            }
        }
        self.offer_deleted.insert(live.offer_id);
        self.offers.remove(&live.offer_id);
        if let Some(ids) = self.owners.get_mut(&live.owner) {
            ids.retain(|id| *id != live.offer_id);
        } else {
            let mut ids = load_owner_ids(self.store, &live.owner)?;
            ids.retain(|id| *id != live.offer_id);
            self.owners.insert(live.owner, ids);
        }
        self.owner_dirty.insert(live.owner);
        let book = live.book();
        let entry = self.books.entry(book.book_key()).or_insert_with(|| {
            let ids = load_book(self.store, book)
                .map(|offers| offers.into_iter().map(|offer| offer.offer_id).collect())
                .unwrap_or_default();
            (book, ids)
        });
        entry.1.retain(|id| *id != live.offer_id);
        self.book_dirty.insert(book.book_key());
        Ok(())
    }

    fn cross_one(
        &mut self,
        taker: &Address,
        taker_gets: DrcBookAsset,
        taker_pays: DrcBookAsset,
        maker: &DrcOfferLive,
        gets_remaining: u64,
        pays_remaining: u64,
    ) -> Result<Cross, StateError> {
        if !agora_types::offers_cross(
            gets_remaining,
            pays_remaining,
            maker.taker_gets_remaining,
            maker.taker_pays_remaining,
        ) {
            return Ok(Cross::Stop);
        }
        let maker_budget = self.deliverable(&maker.owner, maker.taker_gets, true, maker)?;
        if maker_budget == 0 {
            return self.maker_unfunded(maker);
        }
        let taker_budget = self.deliverable(taker, taker_gets, false, maker)?;
        if taker_budget == 0 {
            return Ok(Cross::Stop);
        }
        let want = pays_remaining
            .min(maker.taker_gets_remaining)
            .min(maker_budget);
        let budget = gets_remaining
            .min(maker.taker_pays_remaining)
            .min(taker_budget);
        let Some((received, paid)) = agora_types::offer_fill_step(
            maker.taker_gets_remaining,
            maker.taker_pays_remaining,
            want,
            budget,
        ) else {
            return Ok(Cross::Remove);
        };
        // Probe both legs before moving value so a blocked receive cannot debit the payer.
        let pay_gate = self.transfer(taker, &maker.owner, taker_gets, paid, false, true)?;
        let receive_gate = self.transfer(&maker.owner, taker, taker_pays, received, true, true)?;
        if pay_gate.is_err_skip() {
            return Ok(Cross::Stop);
        }
        if receive_gate.is_err_skip() {
            return self.maker_unfunded(maker);
        }
        self.transfer(taker, &maker.owner, taker_gets, paid, false, false)?;
        self.transfer(&maker.owner, taker, taker_pays, received, true, false)?;
        let mut updated = maker.clone();
        updated.taker_gets_remaining -= received;
        updated.taker_pays_remaining -= paid;
        if updated.taker_gets.is_native() {
            updated.native_locked = updated.native_locked.saturating_sub(received);
        } else if let Some(asset) = updated.taker_gets.issued() {
            if updated.owner != asset.issuer {
                self.sub_reserve(&updated.owner, &asset, received)?;
            }
        }
        self.offers.insert(updated.offer_id, updated);
        Ok(Cross::Fill { paid, received })
    }

    fn maker_unfunded(&mut self, maker: &DrcOfferLive) -> Result<Cross, StateError> {
        if maker.taker_gets.is_native() {
            return Ok(Cross::Remove);
        }
        let Some(asset) = maker.taker_gets.issued() else {
            return Ok(Cross::Remove);
        };
        if maker.owner == asset.issuer {
            return Ok(Cross::Skip);
        }
        let Some(line) = self.line(&maker.owner, &asset)? else {
            return Ok(Cross::Remove);
        };
        let policy = load_drc_issued_asset_policy(self.store, &asset)?;
        let line = normalize_trust_line_live(line, &policy);
        if !line.authorized || line.line_deep_frozen || line.balance.as_units() == 0 {
            return Ok(Cross::Remove);
        }
        Ok(Cross::Skip)
    }

    fn deliverable(
        &mut self,
        owner: &Address,
        asset: DrcBookAsset,
        from_offer_lock: bool,
        maker: &DrcOfferLive,
    ) -> Result<u64, StateError> {
        if asset.is_native() {
            if from_offer_lock && maker.owner == *owner {
                return Ok(maker.native_locked);
            }
            return Ok(self.account(owner)?.balance);
        }
        let Some(issued) = asset.issued() else {
            return Ok(0);
        };
        if *owner == issued.issuer {
            return Ok(u64::MAX);
        }
        let Some(line) = self.line(owner, &issued)? else {
            return Ok(0);
        };
        let reserved = self.reserve_of(owner, &issued)?;
        let balance = line.balance.as_units();
        if from_offer_lock && maker.owner == *owner && maker.taker_gets == asset {
            return Ok(maker.taker_gets_remaining.min(balance));
        }
        Ok(balance.saturating_sub(reserved))
    }

    fn transfer(
        &mut self,
        from: &Address,
        to: &Address,
        asset: DrcBookAsset,
        amount: u64,
        from_native_lock: bool,
        dry: bool,
    ) -> Result<TransferGate, StateError> {
        if amount == 0 {
            return Ok(TransferGate::Done);
        }
        if asset.is_native() {
            if from_native_lock {
                if dry {
                    return Ok(TransferGate::Done);
                }
                self.credit_native(to, amount)?;
            } else {
                if self.account(from)?.balance < amount {
                    return Ok(TransferGate::Skip);
                }
                if dry {
                    return Ok(TransferGate::Done);
                }
                self.debit_native(from, amount)?;
                self.credit_native(to, amount)?;
            }
            return Ok(TransferGate::Done);
        }
        let Some(issued) = asset.issued() else {
            return Err(StateError::InvalidTx("malformed DRC offer asset".into()));
        };
        if *from == issued.issuer && *to == issued.issuer {
            return Err(StateError::InvalidTx(
                "DRC offer cannot move an issued asset to its issuer from the issuer".into(),
            ));
        }
        let policy = load_drc_issued_asset_policy(self.store, &issued)?;
        if *from == issued.issuer {
            let Some(mut line) = self.line(to, &issued)? else {
                return Ok(TransferGate::Skip);
            };
            line = normalize_trust_line_live(line, &policy);
            if issued_movement_allowed(&policy, &line, IssuedMovementKind::Issue).is_err() {
                return Ok(TransferGate::Skip);
            }
            let room = line
                .available_limit()
                .map_err(|e| StateError::InvalidTx(e.to_string()))?;
            if room.as_units() < amount {
                return Ok(TransferGate::Skip);
            }
            if dry {
                return Ok(TransferGate::Done);
            }
            line.balance = line
                .balance
                .checked_add(IssuedAmount::from_units(amount))
                .ok_or_else(|| StateError::InvalidTx("issued balance overflow".into()))?;
            self.lines.insert((*to, issued), line);
            self.line_dirty.insert((*to, issued));
            self.bump_liability(&issued, amount, true)?;
            return Ok(TransferGate::Done);
        }
        if *to == issued.issuer {
            let Some(mut line) = self.line(from, &issued)? else {
                return Ok(TransferGate::Skip);
            };
            line = normalize_trust_line_live(line, &policy);
            if issued_movement_allowed(&policy, &line, IssuedMovementKind::Redeem).is_err() {
                return Ok(TransferGate::Skip);
            }
            if line.balance.as_units() < amount {
                return Ok(TransferGate::Skip);
            }
            if dry {
                return Ok(TransferGate::Done);
            }
            line.balance = line
                .balance
                .checked_sub(IssuedAmount::from_units(amount))
                .ok_or_else(|| StateError::InvalidTx("issued redeem exceeds balance".into()))?;
            self.lines.insert((*from, issued), line);
            self.line_dirty.insert((*from, issued));
            self.bump_liability(&issued, amount, false)?;
            return Ok(TransferGate::Done);
        }
        let Some(mut sender) = self.line(from, &issued)? else {
            return Ok(TransferGate::Skip);
        };
        let Some(mut recipient) = self.line(to, &issued)? else {
            return Ok(TransferGate::Skip);
        };
        sender = normalize_trust_line_live(sender, &policy);
        recipient = normalize_trust_line_live(recipient, &policy);
        if issued_movement_allowed(&policy, &sender, IssuedMovementKind::HolderTransfer).is_err()
            || issued_movement_allowed(&policy, &recipient, IssuedMovementKind::HolderTransfer)
                .is_err()
        {
            return Ok(TransferGate::Skip);
        }
        let room = recipient
            .available_limit()
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        if room.as_units() < amount || sender.balance.as_units() < amount {
            return Ok(TransferGate::Skip);
        }
        if dry {
            return Ok(TransferGate::Done);
        }
        sender.balance = sender
            .balance
            .checked_sub(IssuedAmount::from_units(amount))
            .ok_or_else(|| StateError::InvalidTx("issued transfer exceeds balance".into()))?;
        recipient.balance = recipient
            .balance
            .checked_add(IssuedAmount::from_units(amount))
            .ok_or_else(|| StateError::InvalidTx("issued balance overflow".into()))?;
        self.lines.insert((*from, issued), sender);
        self.lines.insert((*to, issued), recipient);
        self.line_dirty.insert((*from, issued));
        self.line_dirty.insert((*to, issued));
        Ok(TransferGate::Done)
    }

    fn bump_liability(
        &mut self,
        asset: &IssuedAssetId,
        amount: u64,
        issue: bool,
    ) -> Result<(), StateError> {
        if !self.liability.contains_key(asset) {
            self.liability
                .insert(*asset, load_drc_issuer_liability(self.store, asset)?);
        }
        let current = self
            .liability
            .get(asset)
            .copied()
            .unwrap_or(IssuedAmount::ZERO);
        let next = if issue {
            current
                .checked_add(IssuedAmount::from_units(amount))
                .ok_or_else(|| StateError::InvalidTx("issuer liability overflow".into()))?
        } else {
            current
                .checked_sub(IssuedAmount::from_units(amount))
                .ok_or_else(|| StateError::InvalidTx("issuer liability underflow".into()))?
        };
        self.liability.insert(*asset, next);
        self.liability_dirty.insert(*asset);
        Ok(())
    }

    fn flush(
        self,
        batch: &mut WriteBatch,
        journal: &mut AccountJournal,
        payer: &Address,
        sequence_ctx: Option<crate::drc_ticket::DrcSequenceApplyContext>,
        ticket_version: bool,
    ) -> Result<Vec<Vec<u8>>, StateError> {
        let mut meta_keys = Vec::new();
        let mut accounts = self.accounts;
        let originals = self.originals;
        if let Some(account) = accounts.get_mut(payer) {
            if ticket_version {
                if let Some(ctx) = sequence_ctx {
                    finish_drc_account_sequence(
                        batch,
                        payer,
                        account,
                        ctx.consumption,
                        &ctx.tickets_before,
                    )?;
                }
            } else {
                account.nonce = account
                    .nonce
                    .checked_add(1)
                    .ok_or_else(|| StateError::InvalidTx("DRC offer nonce overflow".into()))?;
            }
        }
        for (owner, account) in &accounts {
            if let Some(prior) = originals.get(owner) {
                journal
                    .before
                    .push((NativeAssetId::DRC, *owner, prior.clone()));
            }
            put_account_into(batch, NativeAssetId::DRC, owner, account)?;
        }
        for id in &self.offer_deleted {
            meta_keys.push(offer_live_key(id));
            batch.delete_cf(ColumnFamily::Meta, &offer_live_key(id));
        }
        for (id, live) in &self.offers {
            if self.offer_deleted.contains(id) {
                continue;
            }
            meta_keys.push(offer_live_key(id));
            batch.put_cf(
                ColumnFamily::Meta,
                &offer_live_key(id),
                &borsh::to_vec(live).map_err(|e| StateError::Storage(e.to_string()))?,
            );
        }
        for owner in &self.owner_dirty {
            let key = offer_owner_index_key(owner);
            meta_keys.push(key.clone());
            let ids = self.owners.get(owner).cloned().unwrap_or_default();
            if ids.is_empty() {
                batch.delete_cf(ColumnFamily::Meta, &key);
            } else {
                let mut live_ids = ids;
                live_ids.sort_by_key(|id| id.as_bytes().to_vec());
                let record = OwnerIndex {
                    version: INDEX_VERSION,
                    owner: *owner,
                    live_ids,
                };
                batch.put_cf(
                    ColumnFamily::Meta,
                    &key,
                    &borsh::to_vec(&record).map_err(|e| StateError::Storage(e.to_string()))?,
                );
            }
        }
        for book_key in &self.book_dirty {
            let Some((book, ids)) = self.books.get(book_key) else {
                continue;
            };
            let key = offer_book_index_key(*book);
            meta_keys.push(key.clone());
            if ids.is_empty() {
                batch.delete_cf(ColumnFamily::Meta, &key);
            } else {
                let record = BookIndex {
                    version: INDEX_VERSION,
                    book_key: *book_key,
                    live_ids: ids.clone(),
                };
                batch.put_cf(
                    ColumnFamily::Meta,
                    &key,
                    &borsh::to_vec(&record).map_err(|e| StateError::Storage(e.to_string()))?,
                );
            }
        }
        if self.sequence_dirty {
            meta_keys.push(SEQUENCE_KEY.to_vec());
            batch.put_cf(
                ColumnFamily::Meta,
                SEQUENCE_KEY,
                &self.sequence.to_le_bytes(),
            );
        }
        for (holder, asset) in &self.reserve_dirty {
            let key = reserve_key(holder, asset);
            meta_keys.push(key.clone());
            let amount = self.reserve.get(&(*holder, *asset)).copied().unwrap_or(0);
            if amount == 0 {
                batch.delete_cf(ColumnFamily::Meta, &key);
            } else {
                batch.put_cf(ColumnFamily::Meta, &key, &amount.to_le_bytes());
            }
        }
        for (holder, asset) in &self.line_dirty {
            if let Some(line) = self.lines.get(&(*holder, *asset)) {
                meta_keys.push(trust_line_key(holder, asset));
                put_live(batch, line)?;
            }
        }
        for asset in &self.liability_dirty {
            meta_keys.push(issuer_liability_key(asset));
            let amount = self
                .liability
                .get(asset)
                .copied()
                .unwrap_or(IssuedAmount::ZERO);
            put_liability(batch, asset, amount)?;
        }
        Ok(meta_keys)
    }
}

enum TransferGate {
    Done,
    Skip,
}

impl TransferGate {
    fn is_err_skip(&self) -> bool {
        matches!(self, Self::Skip)
    }
}
