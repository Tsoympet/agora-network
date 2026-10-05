//! Trust lines and issued-value transfers (issuer liabilities; not native DRC).

use agora_types::{
    resolve_drc_account_sequence, Address, Amount, DrcIssuedTransferReceipt, DrcIssuedTransferTx,
    DrcIssuerLiability, DrcTrustLineLive, DrcTrustLineSetTx, Hash, IssuedAmount, IssuedAssetId,
    NativeAssetId, DRC_ISSUED_TRANSFER_RECEIPT_VERSION, DRC_ISSUER_LIABILITY_STATE_VERSION,
    DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER, DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER,
    DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION, DRC_TRUST_LINE_SET_TICKET_VERSION,
};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::{
    verify_drc_issued_transfer_operation, verify_drc_trust_line_set_operation,
};
use crate::drc_policy::load_drc_account_policy;
use crate::drc_ticket::{begin_drc_account_sequence, finish_drc_account_sequence};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const LINE_PREFIX: &[u8] = b"trust/drc/line/";
const HOLDER_INDEX_PREFIX: &[u8] = b"trust/drc/holder/";
const ISSUER_INDEX_PREFIX: &[u8] = b"trust/drc/issuer/";
const LIABILITY_PREFIX: &[u8] = b"trust/drc/liability/";
const TRANSFER_RECEIPT_PREFIX: &[u8] = b"trust/drc/xfer/";

pub const DRC_TRUST_LINE_ROOT_DOMAIN: &[u8] = b"agora-drc-trust-line-root-v1";

pub fn trust_line_key(holder: &Address, asset: &IssuedAssetId) -> Vec<u8> {
    let mut key = Vec::with_capacity(LINE_PREFIX.len() + 20 + 32);
    key.extend_from_slice(LINE_PREFIX);
    key.extend_from_slice(&holder.0);
    key.extend_from_slice(asset.asset_key().as_bytes());
    key
}

pub fn issuer_liability_key(asset: &IssuedAssetId) -> Vec<u8> {
    let mut key = Vec::with_capacity(LIABILITY_PREFIX.len() + 32);
    key.extend_from_slice(LIABILITY_PREFIX);
    key.extend_from_slice(asset.asset_key().as_bytes());
    key
}

pub fn issued_transfer_receipt_key(tx_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(TRANSFER_RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(TRANSFER_RECEIPT_PREFIX);
    key.extend_from_slice(tx_id.as_bytes());
    key
}

pub fn trust_line_meta_keys(holder: &Address, asset: &IssuedAssetId) -> Vec<Vec<u8>> {
    vec![
        trust_line_key(holder, asset),
        holder_index_key(holder),
        issuer_holders_index_key(&asset.issuer, &asset.currency),
        issuer_liability_key(asset),
    ]
}

fn holder_index_key(holder: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(HOLDER_INDEX_PREFIX.len() + 20);
    key.extend_from_slice(HOLDER_INDEX_PREFIX);
    key.extend_from_slice(&holder.0);
    key
}

fn issuer_holders_index_key(
    issuer: &Address,
    currency: &agora_types::IssuedCurrencyCode,
) -> Vec<u8> {
    let mut key = Vec::with_capacity(ISSUER_INDEX_PREFIX.len() + 20 + 20);
    key.extend_from_slice(ISSUER_INDEX_PREFIX);
    key.extend_from_slice(&issuer.0);
    key.extend_from_slice(&currency.0);
    key
}

#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct HolderIndex {
    version: u32,
    holder: Address,
    assets: Vec<Hash>,
}

#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct IssuerHoldersIndex {
    version: u32,
    issuer: Address,
    currency: agora_types::IssuedCurrencyCode,
    holders: Vec<Address>,
}

const INDEX_VERSION: u32 = 1;

pub fn load_drc_trust_line_live(
    store: &StateStore,
    holder: &Address,
    asset: &IssuedAssetId,
) -> Result<Option<DrcTrustLineLive>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &trust_line_key(holder, asset))? else {
        return Ok(None);
    };
    let live =
        DrcTrustLineLive::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    live.validate()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    Ok(Some(live))
}

pub fn load_drc_issuer_liability(
    store: &StateStore,
    asset: &IssuedAssetId,
) -> Result<IssuedAmount, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &issuer_liability_key(asset))? else {
        return Ok(IssuedAmount::ZERO);
    };
    let record = DrcIssuerLiability::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if record.version != DRC_ISSUER_LIABILITY_STATE_VERSION || record.asset != *asset {
        return Err(StateError::Storage(
            "invalid issuer liability record".into(),
        ));
    }
    Ok(record.outstanding)
}

pub fn load_drc_issued_transfer_receipt(
    store: &StateStore,
    tx_id: &Hash,
) -> Result<Option<DrcIssuedTransferReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &issued_transfer_receipt_key(tx_id))? else {
        return Ok(None);
    };
    let receipt = DrcIssuedTransferReceipt::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if receipt.version != DRC_ISSUED_TRANSFER_RECEIPT_VERSION {
        return Err(StateError::Storage(
            "invalid issued transfer receipt".into(),
        ));
    }
    Ok(Some(receipt))
}

fn load_holder_index(store: &StateStore, holder: &Address) -> Result<Vec<Hash>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &holder_index_key(holder))? else {
        return Ok(Vec::new());
    };
    let idx =
        HolderIndex::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))?;
    if idx.version != INDEX_VERSION || idx.holder != *holder {
        return Err(StateError::Storage(
            "invalid trust line holder index".into(),
        ));
    }
    Ok(idx.assets)
}

fn put_holder_index(
    batch: &mut WriteBatch,
    holder: &Address,
    assets: &[Hash],
) -> Result<(), StateError> {
    let key = holder_index_key(holder);
    if assets.is_empty() {
        batch.delete_cf(ColumnFamily::Meta, &key);
        return Ok(());
    }
    if assets.len() > DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER {
        return Err(StateError::InvalidTx(
            "trust line holder cap exceeded".into(),
        ));
    }
    let idx = HolderIndex {
        version: INDEX_VERSION,
        holder: *holder,
        assets: assets.to_vec(),
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &key,
        &borsh::to_vec(&idx).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

fn load_issuer_holders_index(
    store: &StateStore,
    issuer: &Address,
    currency: &agora_types::IssuedCurrencyCode,
) -> Result<Vec<Address>, StateError> {
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &issuer_holders_index_key(issuer, currency),
    )?
    else {
        return Ok(Vec::new());
    };
    let idx = IssuerHoldersIndex::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if idx.version != INDEX_VERSION || idx.issuer != *issuer || idx.currency.0 != currency.0 {
        return Err(StateError::Storage(
            "invalid trust line issuer holders index".into(),
        ));
    }
    Ok(idx.holders)
}

fn put_issuer_holders_index(
    batch: &mut WriteBatch,
    issuer: &Address,
    currency: &agora_types::IssuedCurrencyCode,
    holders: &[Address],
) -> Result<(), StateError> {
    let key = issuer_holders_index_key(issuer, currency);
    if holders.is_empty() {
        batch.delete_cf(ColumnFamily::Meta, &key);
        return Ok(());
    }
    if holders.len() > DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER {
        return Err(StateError::InvalidTx(
            "trust line issuer holder cap exceeded".into(),
        ));
    }
    let idx = IssuerHoldersIndex {
        version: INDEX_VERSION,
        issuer: *issuer,
        currency: *currency,
        holders: holders.to_vec(),
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &key,
        &borsh::to_vec(&idx).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn count_live_trust_lines_for_holder(
    store: &StateStore,
    holder: &Address,
) -> Result<usize, StateError> {
    Ok(load_holder_index(store, holder)?.len())
}

pub fn count_live_trust_line_holders_for_issuer(
    store: &StateStore,
    issuer: &Address,
    currency: &agora_types::IssuedCurrencyCode,
) -> Result<usize, StateError> {
    Ok(load_issuer_holders_index(store, issuer, currency)?.len())
}

pub fn sum_holder_balances_for_asset(
    store: &StateStore,
    asset: &IssuedAssetId,
) -> Result<IssuedAmount, StateError> {
    let holders = load_issuer_holders_index(store, &asset.issuer, &asset.currency)?;
    let mut total = IssuedAmount::ZERO;
    for holder in holders {
        if let Some(line) = load_drc_trust_line_live(store, &holder, asset)? {
            total = total
                .checked_add(line.balance)
                .ok_or_else(|| StateError::Storage("issued balance sum overflow".into()))?;
        }
    }
    Ok(total)
}

pub(crate) fn put_live(batch: &mut WriteBatch, live: &DrcTrustLineLive) -> Result<(), StateError> {
    live.validate()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    batch.put_cf(
        ColumnFamily::Meta,
        &trust_line_key(&live.holder, &live.asset),
        &borsh::to_vec(live).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub(crate) fn put_liability(
    batch: &mut WriteBatch,
    asset: &IssuedAssetId,
    outstanding: IssuedAmount,
) -> Result<(), StateError> {
    if outstanding.is_zero() {
        batch.delete_cf(ColumnFamily::Meta, &issuer_liability_key(asset));
        return Ok(());
    }
    let record = DrcIssuerLiability {
        version: DRC_ISSUER_LIABILITY_STATE_VERSION,
        asset: *asset,
        outstanding,
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &issuer_liability_key(asset),
        &borsh::to_vec(&record).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub(crate) fn debit_drc_fee(
    store: &StateStore,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
    payer: &Address,
    fee: Amount,
    sequence_ctx: Option<crate::drc_ticket::DrcSequenceApplyContext>,
) -> Result<(), StateError> {
    let mut account = load_account(store, NativeAssetId::DRC, payer)?;
    journal
        .before
        .push((NativeAssetId::DRC, *payer, account.clone()));
    if account.balance < fee.as_base_units() {
        return Err(StateError::InvalidTx("insufficient DRC for fee".into()));
    }
    account.balance -= fee.as_base_units();
    if let Some(ctx) = sequence_ctx {
        finish_drc_account_sequence(
            batch,
            payer,
            &mut account,
            ctx.consumption,
            &ctx.tickets_before,
        )?;
    } else {
        account.nonce = account
            .nonce
            .checked_add(1)
            .ok_or_else(|| StateError::InvalidTx("nonce overflow".into()))?;
    }
    put_account_into(batch, NativeAssetId::DRC, payer, &account)?;
    Ok(())
}

pub fn apply_drc_trust_line_set(
    store: &StateStore,
    tx: &DrcTrustLineSetTx,
    auth: &TxAuthContext,
    _application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_trust_line_set_operation(store, tx, auth)?;

    load_account(store, NativeAssetId::DRC, &tx.holder)?;
    load_account(store, NativeAssetId::DRC, &tx.issuer)?;

    let asset = tx.asset_id();
    let sequence_ctx = if tx.version >= DRC_TRUST_LINE_SET_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_TRUST_LINE_SET_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.holder, selector)?)
    } else {
        let acct = load_account(store, NativeAssetId::DRC, &tx.holder)?;
        if acct.nonce != tx.nonce {
            return Err(StateError::InvalidTx("bad trust line set nonce".into()));
        }
        None
    };

    debit_drc_fee(store, batch, journal, &tx.holder, tx.fee, sequence_ctx)?;

    let existing = load_drc_trust_line_live(store, &tx.holder, &asset)?;
    if tx.limit.is_zero() {
        let Some(live) = existing else {
            return Err(StateError::InvalidTx("unknown trust line".into()));
        };
        if !live.balance.is_zero() {
            return Err(StateError::InvalidTx(
                "trust line delete requires zero balance".into(),
            ));
        }
        batch.delete_cf(ColumnFamily::Meta, &trust_line_key(&tx.holder, &asset));
        let mut assets = load_holder_index(store, &tx.holder)?;
        assets.retain(|k| *k != asset.asset_key());
        put_holder_index(batch, &tx.holder, &assets)?;
        let mut holders = load_issuer_holders_index(store, &tx.issuer, &tx.currency)?;
        holders.retain(|h| *h != tx.holder);
        put_issuer_holders_index(batch, &tx.issuer, &tx.currency, &holders)?;
        return Ok(());
    }

    let live = if let Some(mut live) = existing {
        if tx.limit.as_units() < live.balance.as_units() {
            return Err(StateError::InvalidTx("limit below balance".into()));
        }
        live.limit = tx.limit;
        live
    } else {
        let mut assets = load_holder_index(store, &tx.holder)?;
        if assets.len() >= DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER {
            return Err(StateError::InvalidTx(
                "trust line holder cap exceeded".into(),
            ));
        }
        assets.push(asset.asset_key());
        put_holder_index(batch, &tx.holder, &assets)?;
        let mut holders = load_issuer_holders_index(store, &tx.issuer, &tx.currency)?;
        if holders.len() >= DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER {
            return Err(StateError::InvalidTx(
                "trust line issuer holder cap exceeded".into(),
            ));
        }
        if !holders.contains(&tx.holder) {
            holders.push(tx.holder);
            put_issuer_holders_index(batch, &tx.issuer, &tx.currency, &holders)?;
        }
        let policy = crate::drc_issued_controls::load_drc_issued_asset_policy(store, &asset)?;
        DrcTrustLineLive {
            version: agora_types::DRC_TRUST_LINE_LIVE_STATE_V2,
            holder: tx.holder,
            asset,
            limit: tx.limit,
            balance: IssuedAmount::ZERO,
            authorized: !policy.require_auth,
            line_frozen: false,
            line_deep_frozen: false,
        }
    };
    put_live(batch, &live)?;
    Ok(())
}

pub fn apply_drc_issued_transfer(
    store: &StateStore,
    tx: &DrcIssuedTransferTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<(), StateError> {
    tx.validate_structure()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    verify_drc_issued_transfer_operation(store, tx, auth)?;

    let asset = tx.asset_id();
    load_account(store, NativeAssetId::DRC, &tx.sender)?;
    load_account(store, NativeAssetId::DRC, &tx.recipient)?;
    load_account(store, NativeAssetId::DRC, &asset.issuer)?;

    let recipient_policy = load_drc_account_policy(store, &tx.recipient)?;
    if recipient_policy.require_destination_tag && tx.destination_tag.is_none() {
        return Err(StateError::InvalidTx(
            "recipient requires destination tag".into(),
        ));
    }
    if recipient_policy.deposit_auth_required
        && tx.sender != tx.recipient
        && !crate::drc_deposit_preauth::load_drc_deposit_preauth(store, &tx.recipient, &tx.sender)?
    {
        return Err(StateError::InvalidTx(
            "DRC deposit authorization required by recipient policy".into(),
        ));
    }

    let sequence_ctx = if tx.version >= DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION {
        let selector = resolve_drc_account_sequence(
            tx.version,
            DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        Some(begin_drc_account_sequence(store, &tx.sender, selector)?)
    } else {
        let acct = load_account(store, NativeAssetId::DRC, &tx.sender)?;
        if acct.nonce != tx.nonce {
            return Err(StateError::InvalidTx("bad issued transfer nonce".into()));
        }
        None
    };

    debit_drc_fee(store, batch, journal, &tx.sender, tx.fee, sequence_ctx)?;

    let policy = crate::drc_issued_controls::load_drc_issued_asset_policy(store, &asset)?;
    let issuer = asset.issuer;
    let movement_kind = if tx.sender == issuer && tx.recipient != issuer {
        crate::drc_issued_controls::IssuedMovementKind::Issue
    } else if tx.recipient == issuer && tx.sender != issuer {
        crate::drc_issued_controls::IssuedMovementKind::Redeem
    } else if tx.sender != issuer && tx.recipient != issuer {
        crate::drc_issued_controls::IssuedMovementKind::HolderTransfer
    } else {
        return Err(StateError::InvalidTx(
            "invalid issuer transfer endpoints".into(),
        ));
    };
    match movement_kind {
        crate::drc_issued_controls::IssuedMovementKind::Issue => {
            let Some(line) = load_drc_trust_line_live(store, &tx.recipient, &asset)? else {
                return Err(StateError::InvalidTx("recipient missing trust line".into()));
            };
            let line = crate::drc_issued_controls::normalize_trust_line_live(line, &policy);
            crate::drc_issued_controls::issued_movement_allowed(&policy, &line, movement_kind)?;
        }
        crate::drc_issued_controls::IssuedMovementKind::Redeem => {
            let Some(line) = load_drc_trust_line_live(store, &tx.sender, &asset)? else {
                return Err(StateError::InvalidTx("sender missing trust line".into()));
            };
            let line = crate::drc_issued_controls::normalize_trust_line_live(line, &policy);
            crate::drc_issued_controls::issued_movement_allowed(&policy, &line, movement_kind)?;
        }
        crate::drc_issued_controls::IssuedMovementKind::HolderTransfer => {
            let Some(sender_line) = load_drc_trust_line_live(store, &tx.sender, &asset)? else {
                return Err(StateError::InvalidTx("sender missing trust line".into()));
            };
            let Some(recipient_line) = load_drc_trust_line_live(store, &tx.recipient, &asset)?
            else {
                return Err(StateError::InvalidTx("recipient missing trust line".into()));
            };
            let sender_line =
                crate::drc_issued_controls::normalize_trust_line_live(sender_line, &policy);
            let recipient_line =
                crate::drc_issued_controls::normalize_trust_line_live(recipient_line, &policy);
            crate::drc_issued_controls::issued_movement_allowed(
                &policy,
                &sender_line,
                movement_kind,
            )?;
            crate::drc_issued_controls::issued_movement_allowed(
                &policy,
                &recipient_line,
                movement_kind,
            )?;
        }
    }

    let mut liability = load_drc_issuer_liability(store, &asset)?;

    if tx.sender == issuer && tx.recipient != issuer {
        // Issue to holder.
        let mut line = load_drc_trust_line_live(store, &tx.recipient, &asset)?
            .ok_or_else(|| StateError::InvalidTx("recipient missing trust line".into()))?;
        let available = line
            .available_limit()
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        if available.as_units() < tx.amount.as_units() {
            return Err(StateError::InvalidTx("issue exceeds line limit".into()));
        }
        line.balance = line
            .balance
            .checked_add(tx.amount)
            .ok_or_else(|| StateError::InvalidTx("issued balance overflow".into()))?;
        liability = liability
            .checked_add(tx.amount)
            .ok_or_else(|| StateError::InvalidTx("issuer liability overflow".into()))?;
        put_live(batch, &line)?;
    } else if tx.recipient == issuer && tx.sender != issuer {
        // Redeem from holder to issuer.
        let mut line = load_drc_trust_line_live(store, &tx.sender, &asset)?
            .ok_or_else(|| StateError::InvalidTx("sender missing trust line".into()))?;
        line.balance = line
            .balance
            .checked_sub(tx.amount)
            .ok_or_else(|| StateError::InvalidTx("redeem exceeds balance".into()))?;
        liability = liability
            .checked_sub(tx.amount)
            .ok_or_else(|| StateError::InvalidTx("issuer liability underflow".into()))?;
        put_live(batch, &line)?;
    } else if tx.sender != issuer && tx.recipient != issuer {
        // Holder-to-holder transfer.
        let mut sender_line = load_drc_trust_line_live(store, &tx.sender, &asset)?
            .ok_or_else(|| StateError::InvalidTx("sender missing trust line".into()))?;
        let mut recipient_line = load_drc_trust_line_live(store, &tx.recipient, &asset)?
            .ok_or_else(|| StateError::InvalidTx("recipient missing trust line".into()))?;
        sender_line.balance = sender_line
            .balance
            .checked_sub(tx.amount)
            .ok_or_else(|| StateError::InvalidTx("transfer exceeds sender balance".into()))?;
        let available = recipient_line
            .available_limit()
            .map_err(|e| StateError::InvalidTx(e.to_string()))?;
        if available.as_units() < tx.amount.as_units() {
            return Err(StateError::InvalidTx(
                "transfer exceeds recipient limit".into(),
            ));
        }
        recipient_line.balance = recipient_line
            .balance
            .checked_add(tx.amount)
            .ok_or_else(|| StateError::InvalidTx("recipient balance overflow".into()))?;
        put_live(batch, &sender_line)?;
        put_live(batch, &recipient_line)?;
    } else {
        return Err(StateError::InvalidTx(
            "invalid issuer transfer endpoints".into(),
        ));
    }

    put_liability(batch, &asset, liability)?;

    let receipt = DrcIssuedTransferReceipt {
        version: DRC_ISSUED_TRANSFER_RECEIPT_VERSION,
        transfer_tx_id: tx.issued_transfer_tx_id(),
        asset,
        sender: tx.sender,
        recipient: tx.recipient,
        amount: tx.amount,
        source_tag: tx.source_tag,
        destination_tag: tx.destination_tag,
        settlement_blue_score: application_blue_score,
    };
    batch.put_cf(
        ColumnFamily::Meta,
        &issued_transfer_receipt_key(&receipt.transfer_tx_id),
        &borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?,
    );
    Ok(())
}

pub fn trust_line_set_meta_keys(tx: &DrcTrustLineSetTx) -> Vec<Vec<u8>> {
    let asset = tx.asset_id();
    trust_line_meta_keys(&tx.holder, &asset)
}

pub fn issued_transfer_meta_keys(tx: &DrcIssuedTransferTx) -> Vec<Vec<u8>> {
    let asset = tx.asset_id();
    let mut keys = trust_line_meta_keys(&tx.sender, &asset);
    if tx.recipient != tx.sender {
        keys.extend(trust_line_meta_keys(&tx.recipient, &asset));
    }
    keys.push(issued_transfer_receipt_key(&tx.issued_transfer_tx_id()));
    keys
}

pub fn drc_trust_line_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries: Vec<DrcTrustLineLive> = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, LINE_PREFIX)? {
        if key.len() != LINE_PREFIX.len() + 20 + 32 {
            continue;
        }
        let live = DrcTrustLineLive::try_from_slice(&bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        live.validate()
            .map_err(|e| StateError::Storage(e.to_string()))?;
        entries.push(live);
    }
    entries.sort_by(|a, b| {
        a.holder.0.cmp(&b.holder.0).then_with(|| {
            a.asset
                .asset_key()
                .as_bytes()
                .cmp(b.asset.asset_key().as_bytes())
        })
    });
    Ok(Hash::hash_borsh(&(DRC_TRUST_LINE_ROOT_DOMAIN, entries)))
}

pub fn lookup_drc_trust_line_point(
    store: &StateStore,
    holder: &Address,
    asset: &IssuedAssetId,
) -> Result<&'static str, StateError> {
    if load_drc_trust_line_live(store, holder, asset)?.is_some() {
        return Ok("live");
    }
    Ok("unknown")
}

/// Canonical issuer liability only (never enumerates holders).
pub fn lookup_drc_issuer_liability_point(
    store: &StateStore,
    asset: &IssuedAssetId,
) -> Result<(&'static str, IssuedAmount), StateError> {
    asset
        .validate()
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;
    let outstanding = load_drc_issuer_liability(store, asset)?;
    Ok(if outstanding.is_zero() {
        ("unknown", outstanding)
    } else {
        ("live", outstanding)
    })
}
