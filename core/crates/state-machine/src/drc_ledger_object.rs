//! Root-committed identity and bounded query indexes for live DRC ledger objects.
//!
//! The canonical object payload remains in each existing typed state family. This
//! module maintains a fail-closed mirror keyed by a domain-separated object ID and
//! a sparse `(owner, kind, object_id)` index. It never changes historical block or
//! transaction encoding.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use agora_types::{
    Address, Block, DrcAcceptedOperationReceipt, DrcAccountPolicy, DrcAccountRegularKey,
    DrcAccountSequence, DrcAccountSignerList, DrcAccountTickets, DrcCheckLive, DrcDepositPreauth,
    DrcEscrowLive, DrcIssuedAssetPolicyLive, DrcLedgerObject, DrcLedgerObjectDescriptor,
    DrcLedgerObjectKey, DrcLedgerObjectKind, DrcLedgerObjectPage, DrcOperation,
    DrcPaymentChannelLive, DrcTrustLineLive, Hash, NativeAssetId, TransactionAcceptance,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::acceptance::BlockAcceptanceRecord;
use crate::columns::ColumnFamily;
use crate::drc_check::check_live_key;
use crate::drc_deposit_preauth::drc_deposit_preauth_key;
use crate::drc_escrow::escrow_live_key;
use crate::drc_payment_channel::payment_channel_live_key;
use crate::drc_policy::drc_account_policy_key;
use crate::drc_regular_key::drc_regular_key_meta_key;
use crate::drc_signer_list::drc_signer_list_meta_key;
use crate::drc_ticket::drc_ticket_meta_key;
use crate::drc_trust_line::trust_line_key;
use crate::store::WriteBatch;
use crate::supply::{load_schema_version, put_schema_version_into};
use crate::{StateError, StateStore, TxAuthContext, UtxoJournal};

pub const DRC_LEDGER_INDEX_SCHEMA_VERSION: u32 = 1;
pub const DRC_LEDGER_INDEX_DATADIR_SCHEMA: u32 = 21;
pub const DRC_LEDGER_OBJECT_PAGE_MAX: usize = 100;
pub const DRC_LEDGER_OBJECT_INDEX_ROOT_DOMAIN: &[u8] =
    b"agora-trident-drc-ledger-object-index-root-v1";

const INDEX_PREFIX: &[u8] = b"ledger/drc/";
const OBJECT_BY_ID_PREFIX: &[u8] = b"ledger/drc/object/by-id/";
const OBJECT_BY_OWNER_PREFIX: &[u8] = b"ledger/drc/object/by-owner/";
const OPERATION_BY_ID_PREFIX: &[u8] = b"ledger/drc/operation/by-id/";
const OPERATION_BY_TX_PREFIX: &[u8] = b"ledger/drc/operation/by-tx/";
const INDEX_MARKER_KEY: &[u8] = b"ledger/drc/index/version";
const INDEX_MARKER_BYTES: &[u8] = &[1, 0, 0, 0, 21, 0, 0, 0];
const OWNER_CURSOR_DOMAIN: &[u8] = b"agora-trident-drc-owner-cursor-v1";

const POLICY_PREFIX: &[u8] = b"policy/drc/account/";
const DEPOSIT_PREAUTH_PREFIX: &[u8] = b"policy/drc/deposit-preauth/";
const REGULAR_KEY_PREFIX: &[u8] = b"account/drc/regular-key/";
const SIGNER_LIST_PREFIX: &[u8] = b"account/drc/signer-list/";
const TICKET_PREFIX: &[u8] = b"account/drc/tickets/";
const ESCROW_LIVE_PREFIX: &[u8] = b"escrow/drc/live/";
const CHECK_LIVE_PREFIX: &[u8] = b"check/drc/live/";
const PAYMENT_CHANNEL_LIVE_PREFIX: &[u8] = b"paychan/drc/live/";
const TRUST_LINE_PREFIX: &[u8] = b"trust/drc/line/";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct DrcLedgerIndexMarker {
    version: u32,
    datadir_schema: u32,
}

impl DrcLedgerIndexMarker {
    const CURRENT: Self = Self {
        version: DRC_LEDGER_INDEX_SCHEMA_VERSION,
        datadir_schema: DRC_LEDGER_INDEX_DATADIR_SCHEMA,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct OwnerCursor {
    version: u32,
    owner: Address,
    filter: Option<DrcLedgerObjectKind>,
    last_kind: DrcLedgerObjectKind,
    last_object_id: Hash,
    integrity: Hash,
}

impl OwnerCursor {
    fn new(
        owner: Address,
        filter: Option<DrcLedgerObjectKind>,
        last_kind: DrcLedgerObjectKind,
        last_object_id: Hash,
    ) -> Self {
        let version = DRC_LEDGER_INDEX_SCHEMA_VERSION;
        let integrity = Hash::hash_borsh(&(
            OWNER_CURSOR_DOMAIN,
            version,
            owner,
            filter,
            last_kind,
            last_object_id,
        ));
        Self {
            version,
            owner,
            filter,
            last_kind,
            last_object_id,
            integrity,
        }
    }

    fn has_valid_integrity(&self) -> bool {
        self.integrity
            == Hash::hash_borsh(&(
                OWNER_CURSOR_DOMAIN,
                self.version,
                self.owner,
                self.filter,
                self.last_kind,
                self.last_object_id,
            ))
    }
}

fn storage(error: impl ToString) -> StateError {
    StateError::Storage(error.to_string())
}

fn marker_bytes() -> &'static [u8] {
    INDEX_MARKER_BYTES
}

pub fn initialize_drc_ledger_index_into(batch: &mut WriteBatch) {
    batch.put_cf(ColumnFamily::Meta, INDEX_MARKER_KEY, marker_bytes());
}

pub fn verify_drc_ledger_index_ready(store: &StateStore) -> Result<(), StateError> {
    let schema = load_schema_version(store)?;
    if schema < DRC_LEDGER_INDEX_DATADIR_SCHEMA {
        return Err(storage(format!(
            "DRC ledger-object index requires datadir schema {DRC_LEDGER_INDEX_DATADIR_SCHEMA}, found {schema}"
        )));
    }
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, INDEX_MARKER_KEY)? else {
        return Err(storage("DRC ledger-object index marker is missing"));
    };
    let marker = DrcLedgerIndexMarker::try_from_slice(&bytes).map_err(storage)?;
    if marker != DrcLedgerIndexMarker::CURRENT {
        return Err(storage("unsupported DRC ledger-object index marker"));
    }
    Ok(())
}

fn object_by_id_key(object_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(OBJECT_BY_ID_PREFIX.len() + 32);
    key.extend_from_slice(OBJECT_BY_ID_PREFIX);
    key.extend_from_slice(object_id.as_bytes());
    key
}

fn owner_prefix(owner: &Address, kind: Option<DrcLedgerObjectKind>) -> Vec<u8> {
    let mut key =
        Vec::with_capacity(OBJECT_BY_OWNER_PREFIX.len() + 20 + usize::from(kind.is_some()));
    key.extend_from_slice(OBJECT_BY_OWNER_PREFIX);
    key.extend_from_slice(&owner.0);
    if let Some(kind) = kind {
        key.push(kind.wire_byte());
    }
    key
}

fn object_by_owner_key(owner: &Address, kind: DrcLedgerObjectKind, object_id: &Hash) -> Vec<u8> {
    let mut key = owner_prefix(owner, Some(kind));
    key.extend_from_slice(object_id.as_bytes());
    key
}

fn operation_by_id_key(operation_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(OPERATION_BY_ID_PREFIX.len() + 32);
    key.extend_from_slice(OPERATION_BY_ID_PREFIX);
    key.extend_from_slice(operation_id.as_bytes());
    key
}

fn operation_by_tx_key(transaction_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(OPERATION_BY_TX_PREFIX.len() + 32);
    key.extend_from_slice(OPERATION_BY_TX_PREFIX);
    key.extend_from_slice(transaction_id.as_bytes());
    key
}

fn source_key(key: &DrcLedgerObjectKey) -> Vec<u8> {
    match key {
        DrcLedgerObjectKey::AccountPolicy { account } => drc_account_policy_key(account),
        DrcLedgerObjectKey::DepositPreauthorization {
            owner,
            authorized_source,
        } => drc_deposit_preauth_key(owner, authorized_source),
        DrcLedgerObjectKey::RegularKey { owner } => drc_regular_key_meta_key(owner),
        DrcLedgerObjectKey::SignerList { owner } => drc_signer_list_meta_key(owner),
        DrcLedgerObjectKey::TicketSet { owner } => drc_ticket_meta_key(owner),
        DrcLedgerObjectKey::Escrow { escrow_id } => escrow_live_key(escrow_id),
        DrcLedgerObjectKey::Check { check_id } => check_live_key(check_id),
        DrcLedgerObjectKey::PaymentChannel { channel_id } => payment_channel_live_key(channel_id),
        DrcLedgerObjectKey::TrustLine { holder, asset } => trust_line_key(holder, asset),
        DrcLedgerObjectKey::IssuedAssetPolicy { asset } => {
            agora_types::drc_issued_asset_policy_meta_key(asset)
        }
    }
}

fn exact_suffix<const N: usize>(key: &[u8], prefix: &[u8]) -> Result<[u8; N], StateError> {
    if key.len() != prefix.len() + N || !key.starts_with(prefix) {
        return Err(storage("malformed DRC ledger-object source key"));
    }
    let mut suffix = [0u8; N];
    suffix.copy_from_slice(&key[prefix.len()..]);
    Ok(suffix)
}

fn decode_source_object(key: &[u8], bytes: &[u8]) -> Result<Option<DrcLedgerObject>, StateError> {
    let object = if key.starts_with(POLICY_PREFIX) {
        let account = Address(exact_suffix::<20>(key, POLICY_PREFIX)?);
        let policy = DrcAccountPolicy::try_from_slice(bytes).map_err(storage)?;
        policy.validate().map_err(storage)?;
        DrcLedgerObject::AccountPolicy { account, policy }
    } else if key.starts_with(DEPOSIT_PREAUTH_PREFIX) {
        let record = DrcDepositPreauth::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if drc_deposit_preauth_key(&record.owner, &record.authorized_source) != key {
            return Err(storage("DRC deposit-preauthorization source key mismatch"));
        }
        DrcLedgerObject::DepositPreauthorization(record)
    } else if key.starts_with(REGULAR_KEY_PREFIX) {
        let record = DrcAccountRegularKey::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if drc_regular_key_meta_key(&record.owner) != key {
            return Err(storage("DRC regular-key source key mismatch"));
        }
        DrcLedgerObject::RegularKey(record)
    } else if key.starts_with(SIGNER_LIST_PREFIX) {
        let record = DrcAccountSignerList::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if drc_signer_list_meta_key(&record.owner) != key {
            return Err(storage("DRC signer-list source key mismatch"));
        }
        DrcLedgerObject::SignerList(record)
    } else if key.starts_with(TICKET_PREFIX) {
        let record = DrcAccountTickets::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if drc_ticket_meta_key(&record.owner) != key {
            return Err(storage("DRC ticket-set source key mismatch"));
        }
        DrcLedgerObject::TicketSet(record)
    } else if key.starts_with(ESCROW_LIVE_PREFIX) {
        let record = DrcEscrowLive::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if escrow_live_key(&record.escrow_id) != key {
            return Err(storage("DRC escrow source key mismatch"));
        }
        DrcLedgerObject::Escrow(record)
    } else if key.starts_with(CHECK_LIVE_PREFIX) {
        let record = DrcCheckLive::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if check_live_key(&record.check_id) != key {
            return Err(storage("DRC Check source key mismatch"));
        }
        DrcLedgerObject::Check(record)
    } else if key.starts_with(PAYMENT_CHANNEL_LIVE_PREFIX) {
        let record = DrcPaymentChannelLive::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if payment_channel_live_key(&record.channel_id) != key {
            return Err(storage("DRC payment-channel source key mismatch"));
        }
        DrcLedgerObject::PaymentChannel(record)
    } else if key.starts_with(TRUST_LINE_PREFIX) {
        let record = DrcTrustLineLive::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if trust_line_key(&record.holder, &record.asset) != key {
            return Err(storage("DRC trust-line source key mismatch"));
        }
        DrcLedgerObject::TrustLine(record)
    } else if key.starts_with(agora_types::DRC_ISSUED_ASSET_POLICY_META_PREFIX) {
        let record = DrcIssuedAssetPolicyLive::try_from_slice(bytes).map_err(storage)?;
        record.validate().map_err(storage)?;
        if agora_types::drc_issued_asset_policy_meta_key(&record.asset) != key {
            return Err(storage("DRC issued-policy source key mismatch"));
        }
        DrcLedgerObject::IssuedAssetPolicy(record)
    } else {
        return Ok(None);
    };
    Ok(Some(object))
}

fn load_source_object(
    store: &StateStore,
    key: &DrcLedgerObjectKey,
) -> Result<Option<DrcLedgerObject>, StateError> {
    let source = source_key(key);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &source)? else {
        return Ok(None);
    };
    let object = decode_source_object(&source, &bytes)?
        .ok_or_else(|| storage("unsupported DRC ledger-object source key"))?;
    if object.key() != *key {
        return Err(storage("DRC ledger-object source identity mismatch"));
    }
    Ok(Some(object))
}

fn decode_descriptor(bytes: &[u8]) -> Result<DrcLedgerObjectDescriptor, StateError> {
    let descriptor = DrcLedgerObjectDescriptor::try_from_slice(bytes).map_err(storage)?;
    if !descriptor.is_consistent() {
        return Err(storage("inconsistent DRC ledger-object descriptor"));
    }
    Ok(descriptor)
}

fn load_descriptor_unchecked(
    store: &StateStore,
    object_id: &Hash,
) -> Result<Option<DrcLedgerObjectDescriptor>, StateError> {
    store
        .get_cf(ColumnFamily::Meta, &object_by_id_key(object_id))?
        .map(|bytes| decode_descriptor(&bytes))
        .transpose()
}

pub fn load_drc_ledger_object(
    store: &StateStore,
    object_id: &Hash,
) -> Result<Option<DrcLedgerObjectDescriptor>, StateError> {
    verify_drc_ledger_index_ready(store)?;
    let Some(descriptor) = load_descriptor_unchecked(store, object_id)? else {
        return Ok(None);
    };
    let source = load_source_object(store, &descriptor.key)?
        .ok_or_else(|| storage("indexed DRC ledger object has no live source"))?;
    if source != descriptor.object {
        return Err(storage(
            "indexed DRC ledger object differs from live source",
        ));
    }
    let owner_key = object_by_owner_key(&descriptor.owner, descriptor.kind, object_id);
    if store.get_cf(ColumnFamily::Meta, &owner_key)?.as_deref() != Some(marker_bytes()) {
        return Err(storage(
            "DRC ledger-object owner index is missing or malformed",
        ));
    }
    Ok(Some(descriptor))
}

fn kind_from_wire(value: u8) -> Option<DrcLedgerObjectKind> {
    DrcLedgerObjectKind::ALL
        .into_iter()
        .find(|kind| kind.wire_byte() == value)
}

fn decode_owner_row(key: &[u8]) -> Result<(Address, DrcLedgerObjectKind, Hash), StateError> {
    let expected = OBJECT_BY_OWNER_PREFIX.len() + 20 + 1 + 32;
    if key.len() != expected || !key.starts_with(OBJECT_BY_OWNER_PREFIX) {
        return Err(storage("malformed DRC ledger-object owner-index key"));
    }
    let owner_start = OBJECT_BY_OWNER_PREFIX.len();
    let mut owner = [0u8; 20];
    owner.copy_from_slice(&key[owner_start..owner_start + 20]);
    let kind = kind_from_wire(key[owner_start + 20])
        .ok_or_else(|| storage("unknown DRC ledger-object kind in owner index"))?;
    let mut object_id = [0u8; 32];
    object_id.copy_from_slice(&key[owner_start + 21..]);
    Ok((Address(owner), kind, Hash(object_id)))
}

fn encode_cursor(cursor: &OwnerCursor) -> Result<String, StateError> {
    if !cursor.has_valid_integrity() {
        return Err(storage("refusing to encode malformed DRC owner cursor"));
    }
    borsh::to_vec(cursor).map(hex::encode).map_err(storage)
}

fn decode_cursor(
    value: &str,
    owner: Address,
    filter: Option<DrcLedgerObjectKind>,
) -> Result<OwnerCursor, StateError> {
    let bytes = hex::decode(value).map_err(|_| StateError::InvalidTx("malformed cursor".into()))?;
    let cursor = OwnerCursor::try_from_slice(&bytes)
        .map_err(|_| StateError::InvalidTx("malformed cursor".into()))?;
    if !cursor.has_valid_integrity()
        || cursor.version != DRC_LEDGER_INDEX_SCHEMA_VERSION
        || cursor.owner != owner
        || cursor.filter != filter
        || filter.is_some_and(|kind| cursor.last_kind != kind)
    {
        return Err(StateError::InvalidTx(
            "cursor does not match DRC account-object query".into(),
        ));
    }
    Ok(cursor)
}

pub fn list_drc_account_objects(
    store: &StateStore,
    owner: Address,
    kind: Option<DrcLedgerObjectKind>,
    limit: usize,
    cursor: Option<&str>,
) -> Result<DrcLedgerObjectPage, StateError> {
    verify_drc_ledger_index_ready(store)?;
    if !(1..=DRC_LEDGER_OBJECT_PAGE_MAX).contains(&limit) {
        return Err(StateError::InvalidTx(format!(
            "DRC account-object limit must be 1..={DRC_LEDGER_OBJECT_PAGE_MAX}"
        )));
    }
    let prefix = owner_prefix(&owner, kind);
    let start_key = cursor
        .map(|value| decode_cursor(value, owner, kind))
        .transpose()?
        .map(|decoded| object_by_owner_key(&owner, decoded.last_kind, &decoded.last_object_id));
    let rows = store.scan_prefix_after_limit(
        ColumnFamily::Meta,
        &prefix,
        start_key.as_deref(),
        limit + 1,
    )?;
    let has_more = rows.len() > limit;
    let mut objects = Vec::with_capacity(rows.len().min(limit));
    let marker = marker_bytes();
    for (key, value) in rows.iter().take(limit) {
        if value.as_slice() != marker {
            return Err(storage("malformed DRC ledger-object owner-index value"));
        }
        let (indexed_owner, indexed_kind, object_id) = decode_owner_row(key)?;
        if indexed_owner != owner || kind.is_some_and(|filter| filter != indexed_kind) {
            return Err(storage(
                "DRC ledger-object owner index escaped query prefix",
            ));
        }
        let descriptor = load_drc_ledger_object(store, &object_id)?
            .ok_or_else(|| storage("DRC owner index references a missing object"))?;
        if descriptor.owner != owner || descriptor.kind != indexed_kind {
            return Err(storage("DRC owner index descriptor mismatch"));
        }
        objects.push(descriptor);
    }
    let next_cursor = if has_more {
        let last = objects
            .last()
            .ok_or_else(|| storage("DRC pagination produced an empty continuation"))?;
        Some(encode_cursor(&OwnerCursor::new(
            owner,
            kind,
            last.kind,
            last.object_id,
        ))?)
    } else {
        None
    };
    Ok(DrcLedgerObjectPage {
        owner,
        kind,
        objects,
        next_cursor,
    })
}

fn decode_receipt(bytes: &[u8]) -> Result<DrcAcceptedOperationReceipt, StateError> {
    let receipt = DrcAcceptedOperationReceipt::try_from_slice(bytes).map_err(storage)?;
    if !receipt.is_consistent() {
        return Err(storage("inconsistent DRC accepted-operation receipt"));
    }
    Ok(receipt)
}

pub fn load_drc_operation(
    store: &StateStore,
    operation_id: &Hash,
) -> Result<Option<DrcAcceptedOperationReceipt>, StateError> {
    verify_drc_ledger_index_ready(store)?;
    store
        .get_cf(ColumnFamily::Meta, &operation_by_id_key(operation_id))?
        .map(|bytes| decode_receipt(&bytes))
        .transpose()
}

pub fn load_drc_transaction(
    store: &StateStore,
    historical_transaction_id: &Hash,
) -> Result<Option<DrcAcceptedOperationReceipt>, StateError> {
    verify_drc_ledger_index_ready(store)?;
    let Some(operation_id) = store.get_cf(
        ColumnFamily::Meta,
        &operation_by_tx_key(historical_transaction_id),
    )?
    else {
        return Ok(None);
    };
    if operation_id.len() != 32 {
        return Err(storage("malformed DRC transaction-to-operation index"));
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&operation_id);
    let receipt = load_drc_operation(store, &Hash(id))?
        .ok_or_else(|| storage("DRC transaction index references a missing receipt"))?;
    if receipt.historical_transaction_id != *historical_transaction_id {
        return Err(storage("DRC transaction index receipt mismatch"));
    }
    Ok(Some(receipt))
}

fn collect_source_descriptors(
    store: &StateStore,
) -> Result<BTreeMap<Hash, DrcLedgerObjectDescriptor>, StateError> {
    let mut descriptors = BTreeMap::new();
    for prefix in [
        POLICY_PREFIX,
        DEPOSIT_PREAUTH_PREFIX,
        REGULAR_KEY_PREFIX,
        SIGNER_LIST_PREFIX,
        TICKET_PREFIX,
        ESCROW_LIVE_PREFIX,
        CHECK_LIVE_PREFIX,
        PAYMENT_CHANNEL_LIVE_PREFIX,
        TRUST_LINE_PREFIX,
        agora_types::DRC_ISSUED_ASSET_POLICY_META_PREFIX,
    ] {
        for (key, value) in store.scan_prefix(ColumnFamily::Meta, prefix)? {
            let object = decode_source_object(&key, &value)?
                .ok_or_else(|| storage("unsupported DRC source prefix"))?;
            let descriptor = DrcLedgerObjectDescriptor::new(object);
            if descriptors
                .insert(descriptor.object_id, descriptor)
                .is_some()
            {
                return Err(storage("duplicate deterministic DRC ledger-object ID"));
            }
        }
    }
    Ok(descriptors)
}

pub fn verify_drc_ledger_object_index(store: &StateStore) -> Result<(), StateError> {
    verify_drc_ledger_index_ready(store)?;
    let expected = collect_source_descriptors(store)?;
    let marker = marker_bytes();
    let mut actual = BTreeMap::new();
    for (key, value) in store.scan_prefix(ColumnFamily::Meta, OBJECT_BY_ID_PREFIX)? {
        let id = Hash(exact_suffix::<32>(&key, OBJECT_BY_ID_PREFIX)?);
        let descriptor = decode_descriptor(&value)?;
        if descriptor.object_id != id {
            return Err(storage("DRC descriptor key does not match object ID"));
        }
        if actual.insert(id, descriptor).is_some() {
            return Err(storage("duplicate DRC object descriptor"));
        }
    }
    if actual != expected {
        return Err(storage(
            "DRC object descriptors do not exactly match canonical live state",
        ));
    }

    let expected_owner: BTreeSet<Vec<u8>> = expected
        .values()
        .map(|descriptor| {
            object_by_owner_key(&descriptor.owner, descriptor.kind, &descriptor.object_id)
        })
        .collect();
    let mut actual_owner = BTreeSet::new();
    for (key, value) in store.scan_prefix(ColumnFamily::Meta, OBJECT_BY_OWNER_PREFIX)? {
        let _ = decode_owner_row(&key)?;
        if value.as_slice() != marker {
            return Err(storage("malformed DRC owner-index value"));
        }
        actual_owner.insert(key);
    }
    if actual_owner != expected_owner {
        return Err(storage(
            "DRC owner index does not exactly match canonical live state",
        ));
    }

    let mut expected_tx = BTreeMap::new();
    for (key, value) in store.scan_prefix(ColumnFamily::Meta, OPERATION_BY_ID_PREFIX)? {
        let operation_id = Hash(exact_suffix::<32>(&key, OPERATION_BY_ID_PREFIX)?);
        let receipt = decode_receipt(&value)?;
        if receipt.operation_id != operation_id {
            return Err(storage("DRC receipt key does not match operation ID"));
        }
        if expected_tx
            .insert(receipt.historical_transaction_id, operation_id)
            .is_some()
        {
            return Err(storage(
                "ambiguous historical DRC transaction ID across operation kinds",
            ));
        }
    }
    let mut actual_tx = BTreeMap::new();
    for (key, value) in store.scan_prefix(ColumnFamily::Meta, OPERATION_BY_TX_PREFIX)? {
        let transaction_id = Hash(exact_suffix::<32>(&key, OPERATION_BY_TX_PREFIX)?);
        if value.len() != 32 {
            return Err(storage("malformed DRC transaction-to-operation value"));
        }
        let mut operation_id = [0u8; 32];
        operation_id.copy_from_slice(&value);
        actual_tx.insert(transaction_id, Hash(operation_id));
    }
    if actual_tx != expected_tx {
        return Err(storage(
            "DRC transaction index does not exactly match accepted receipts",
        ));
    }
    Ok(())
}

pub fn drc_ledger_object_index_root(store: &StateStore) -> Result<Hash, StateError> {
    if load_schema_version(store)? < DRC_LEDGER_INDEX_DATADIR_SCHEMA {
        return Ok(Hash::hash_borsh(&(
            DRC_LEDGER_OBJECT_INDEX_ROOT_DOMAIN,
            DRC_LEDGER_INDEX_SCHEMA_VERSION,
            Vec::<DrcLedgerObjectDescriptor>::new(),
            Vec::<DrcAcceptedOperationReceipt>::new(),
        )));
    }
    verify_drc_ledger_object_index(store)?;
    let mut descriptors = Vec::new();
    for (_, value) in store.scan_prefix(ColumnFamily::Meta, OBJECT_BY_ID_PREFIX)? {
        descriptors.push(decode_descriptor(&value)?);
    }
    let mut receipts = Vec::new();
    for (_, value) in store.scan_prefix(ColumnFamily::Meta, OPERATION_BY_ID_PREFIX)? {
        receipts.push(decode_receipt(&value)?);
    }
    Ok(Hash::hash_borsh(&(
        DRC_LEDGER_OBJECT_INDEX_ROOT_DOMAIN,
        DRC_LEDGER_INDEX_SCHEMA_VERSION,
        descriptors,
        receipts,
    )))
}

fn accepted<T: Clone>(
    values: &[T],
    statuses: &[TransactionAcceptance],
    lane: &str,
    map: impl Fn(T) -> DrcOperation,
) -> Result<Vec<DrcOperation>, StateError> {
    if values.len() != statuses.len() {
        return Err(storage(format!(
            "DRC receipt indexing found misaligned {lane} acceptance"
        )));
    }
    Ok(values
        .iter()
        .cloned()
        .zip(statuses)
        .filter(|(_, status)| **status == TransactionAcceptance::Accepted)
        .map(|(value, _)| map(value))
        .collect())
}

fn collect_accepted_operations(
    block: &Block,
    acceptance: &BlockAcceptanceRecord,
) -> Result<Vec<DrcOperation>, StateError> {
    let mut operations = Vec::new();
    if block.account_transfers.len() != acceptance.account_statuses.len()
        || block.stake_ops.len() != acceptance.stake_statuses.len()
    {
        return Err(storage(
            "DRC receipt indexing found misaligned account/stake acceptance",
        ));
    }
    operations.extend(
        block
            .account_transfers
            .iter()
            .cloned()
            .zip(&acceptance.account_statuses)
            .filter(|(tx, status)| {
                tx.asset == NativeAssetId::DRC && **status == TransactionAcceptance::Accepted
            })
            .map(|(tx, _)| DrcOperation::AccountTransfer(tx)),
    );
    operations.extend(
        block
            .stake_ops
            .iter()
            .cloned()
            .zip(&acceptance.stake_statuses)
            .filter(|(tx, status)| {
                tx.asset == NativeAssetId::DRC && **status == TransactionAcceptance::Accepted
            })
            .map(|(tx, _)| DrcOperation::Stake(tx)),
    );
    operations.extend(accepted(
        &block.drc_payments,
        &acceptance.payment_statuses,
        "payment",
        DrcOperation::Payment,
    )?);
    operations.extend(accepted(
        &block.drc_account_policies,
        &acceptance.drc_policy_statuses,
        "account-policy",
        DrcOperation::AccountPolicy,
    )?);
    operations.extend(accepted(
        &block.drc_deposit_preauths,
        &acceptance.drc_deposit_preauth_statuses,
        "deposit-preauthorization",
        DrcOperation::DepositPreauthorization,
    )?);
    operations.extend(accepted(
        &block.drc_regular_keys,
        &acceptance.drc_regular_key_statuses,
        "regular-key",
        DrcOperation::RegularKey,
    )?);
    operations.extend(accepted(
        &block.drc_signer_lists,
        &acceptance.drc_signer_list_statuses,
        "signer-list",
        DrcOperation::SignerList,
    )?);
    operations.extend(accepted(
        &block.drc_ticket_creates,
        &acceptance.drc_ticket_create_statuses,
        "ticket-create",
        DrcOperation::TicketCreate,
    )?);
    operations.extend(accepted(
        &block.drc_escrow_creates,
        &acceptance.drc_escrow_create_statuses,
        "escrow-create",
        DrcOperation::EscrowCreate,
    )?);
    operations.extend(accepted(
        &block.drc_escrow_finishes,
        &acceptance.drc_escrow_finish_statuses,
        "escrow-finish",
        DrcOperation::EscrowFinish,
    )?);
    operations.extend(accepted(
        &block.drc_escrow_cancels,
        &acceptance.drc_escrow_cancel_statuses,
        "escrow-cancel",
        DrcOperation::EscrowCancel,
    )?);
    operations.extend(accepted(
        &block.drc_check_creates,
        &acceptance.drc_check_create_statuses,
        "check-create",
        DrcOperation::CheckCreate,
    )?);
    operations.extend(accepted(
        &block.drc_check_cashes,
        &acceptance.drc_check_cash_statuses,
        "check-cash",
        DrcOperation::CheckCash,
    )?);
    operations.extend(accepted(
        &block.drc_check_cancels,
        &acceptance.drc_check_cancel_statuses,
        "check-cancel",
        DrcOperation::CheckCancel,
    )?);
    operations.extend(accepted(
        &block.drc_payment_channel_creates,
        &acceptance.drc_payment_channel_create_statuses,
        "payment-channel-create",
        DrcOperation::PaymentChannelCreate,
    )?);
    operations.extend(accepted(
        &block.drc_payment_channel_funds,
        &acceptance.drc_payment_channel_fund_statuses,
        "payment-channel-fund",
        DrcOperation::PaymentChannelFund,
    )?);
    operations.extend(accepted(
        &block.drc_payment_channel_claims,
        &acceptance.drc_payment_channel_claim_statuses,
        "payment-channel-claim",
        DrcOperation::PaymentChannelClaim,
    )?);
    operations.extend(accepted(
        &block.drc_payment_channel_closes,
        &acceptance.drc_payment_channel_close_statuses,
        "payment-channel-close",
        DrcOperation::PaymentChannelClose,
    )?);
    operations.extend(accepted(
        &block.drc_trust_line_sets,
        &acceptance.drc_trust_line_set_statuses,
        "trust-line-set",
        DrcOperation::TrustLineSet,
    )?);
    operations.extend(accepted(
        &block.drc_issued_transfers,
        &acceptance.drc_issued_transfer_statuses,
        "issued-transfer",
        DrcOperation::IssuedTransfer,
    )?);
    operations.extend(accepted(
        &block.drc_issued_asset_policy_sets,
        &acceptance.drc_issued_asset_policy_set_statuses,
        "issued-policy",
        DrcOperation::IssuedAssetPolicySet,
    )?);
    operations.extend(accepted(
        &block.drc_trust_line_issuer_controls,
        &acceptance.drc_trust_line_issuer_control_statuses,
        "trust-line-issuer-control",
        DrcOperation::TrustLineIssuerControl,
    )?);
    operations.extend(accepted(
        &block.drc_issued_clawbacks,
        &acceptance.drc_issued_clawback_statuses,
        "issued-clawback",
        DrcOperation::IssuedClawback,
    )?);
    Ok(operations)
}

fn operation_receipt_block<'a>(
    block: &'a Block,
    auth: Option<&TxAuthContext>,
) -> Result<Cow<'a, Block>, StateError> {
    match auth {
        Some(ctx) => {
            agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
                .map_err(|error| StateError::InvalidTx(error.to_string()))?;
            if block.drc_multisign_attachments.is_empty() {
                Ok(Cow::Borrowed(block))
            } else {
                agora_types::merge_drc_multisign_attachments(
                    block.clone(),
                    &ctx.chain_id,
                    &ctx.genesis,
                )
                .map(Cow::Owned)
                .map_err(|error| StateError::InvalidTx(error.to_string()))
            }
        }
        None if block.drc_multisign_attachments.is_empty() => Ok(Cow::Borrowed(block)),
        None => Err(StateError::InvalidTx(
            "DRC accepted-operation indexing requires network-bound attachment auth".into(),
        )),
    }
}

fn direct_operation_object_keys(operation: &DrcOperation) -> Vec<DrcLedgerObjectKey> {
    match operation {
        DrcOperation::AccountTransfer(_) | DrcOperation::Stake(_) | DrcOperation::Payment(_) => {
            Vec::new()
        }
        DrcOperation::AccountPolicy(tx) => {
            vec![DrcLedgerObjectKey::AccountPolicy {
                account: tx.account,
            }]
        }
        DrcOperation::DepositPreauthorization(tx) => {
            vec![DrcLedgerObjectKey::DepositPreauthorization {
                owner: tx.owner,
                authorized_source: tx.authorized_source,
            }]
        }
        DrcOperation::RegularKey(tx) => {
            vec![DrcLedgerObjectKey::RegularKey { owner: tx.owner }]
        }
        DrcOperation::SignerList(tx) => {
            vec![DrcLedgerObjectKey::SignerList { owner: tx.owner }]
        }
        DrcOperation::TicketCreate(tx) => {
            vec![DrcLedgerObjectKey::TicketSet { owner: tx.owner }]
        }
        DrcOperation::EscrowCreate(tx) => {
            vec![DrcLedgerObjectKey::Escrow {
                escrow_id: tx.escrow_id(),
            }]
        }
        DrcOperation::EscrowFinish(tx) => {
            vec![DrcLedgerObjectKey::Escrow {
                escrow_id: tx.escrow_id,
            }]
        }
        DrcOperation::EscrowCancel(tx) => {
            vec![DrcLedgerObjectKey::Escrow {
                escrow_id: tx.escrow_id,
            }]
        }
        DrcOperation::CheckCreate(tx) => {
            vec![DrcLedgerObjectKey::Check {
                check_id: tx.check_id(),
            }]
        }
        DrcOperation::CheckCash(tx) => {
            vec![DrcLedgerObjectKey::Check {
                check_id: tx.check_id,
            }]
        }
        DrcOperation::CheckCancel(tx) => {
            vec![DrcLedgerObjectKey::Check {
                check_id: tx.check_id,
            }]
        }
        DrcOperation::PaymentChannelCreate(tx) => {
            vec![DrcLedgerObjectKey::PaymentChannel {
                channel_id: tx.channel_id(),
            }]
        }
        DrcOperation::PaymentChannelFund(tx) => {
            vec![DrcLedgerObjectKey::PaymentChannel {
                channel_id: tx.channel_id,
            }]
        }
        DrcOperation::PaymentChannelClaim(tx) => {
            vec![DrcLedgerObjectKey::PaymentChannel {
                channel_id: tx.channel_id,
            }]
        }
        DrcOperation::PaymentChannelClose(tx) => {
            vec![DrcLedgerObjectKey::PaymentChannel {
                channel_id: tx.channel_id,
            }]
        }
        DrcOperation::TrustLineSet(tx) => {
            vec![DrcLedgerObjectKey::TrustLine {
                holder: tx.holder,
                asset: tx.asset_id(),
            }]
        }
        DrcOperation::IssuedTransfer(tx) => {
            let asset = tx.asset_id();
            let mut keys = vec![DrcLedgerObjectKey::TrustLine {
                holder: tx.sender,
                asset,
            }];
            if tx.recipient != tx.sender {
                keys.push(DrcLedgerObjectKey::TrustLine {
                    holder: tx.recipient,
                    asset,
                });
            }
            keys
        }
        DrcOperation::IssuedAssetPolicySet(tx) => {
            vec![DrcLedgerObjectKey::IssuedAssetPolicy {
                asset: tx.asset_id(),
            }]
        }
        DrcOperation::TrustLineIssuerControl(tx) => {
            vec![DrcLedgerObjectKey::TrustLine {
                holder: tx.holder,
                asset: tx.asset_id(),
            }]
        }
        DrcOperation::IssuedClawback(tx) => {
            vec![DrcLedgerObjectKey::TrustLine {
                holder: tx.holder,
                asset: tx.asset_id(),
            }]
        }
    }
}

fn ticket_owner_if_consumed(operation: &DrcOperation) -> Option<Address> {
    let selected = |selector: Option<agora_types::DrcAccountSequenceSelector>| {
        selector.is_some_and(|value| value.kind == DrcAccountSequence::Ticket)
    };
    match operation {
        DrcOperation::AccountTransfer(tx) if selected(tx.account_sequence) => Some(tx.from),
        DrcOperation::Stake(tx) if selected(tx.account_sequence) => Some(tx.actor),
        DrcOperation::Payment(tx) if selected(tx.account_sequence) => Some(tx.from),
        DrcOperation::AccountPolicy(tx) if selected(tx.account_sequence) => Some(tx.account),
        DrcOperation::DepositPreauthorization(tx) if selected(tx.account_sequence) => {
            Some(tx.owner)
        }
        DrcOperation::RegularKey(tx) if selected(tx.account_sequence) => Some(tx.owner),
        DrcOperation::SignerList(tx) if selected(tx.account_sequence) => Some(tx.owner),
        DrcOperation::EscrowCreate(tx) if selected(tx.account_sequence) => Some(tx.owner),
        DrcOperation::EscrowFinish(tx) if selected(tx.account_sequence) => Some(tx.submitter),
        DrcOperation::EscrowCancel(tx) if selected(tx.account_sequence) => Some(tx.submitter),
        DrcOperation::CheckCreate(tx) if selected(tx.account_sequence) => Some(tx.owner),
        DrcOperation::CheckCash(tx) if selected(tx.account_sequence) => Some(tx.submitter),
        DrcOperation::CheckCancel(tx) if selected(tx.account_sequence) => Some(tx.submitter),
        DrcOperation::PaymentChannelCreate(tx) if selected(tx.account_sequence) => Some(tx.owner),
        DrcOperation::PaymentChannelFund(tx) if selected(tx.account_sequence) => Some(tx.submitter),
        DrcOperation::PaymentChannelClaim(tx) if selected(tx.account_sequence) => {
            Some(tx.submitter)
        }
        DrcOperation::PaymentChannelClose(tx) if selected(tx.account_sequence) => {
            Some(tx.submitter)
        }
        DrcOperation::TrustLineSet(tx) if selected(tx.account_sequence) => Some(tx.holder),
        DrcOperation::IssuedTransfer(tx) if selected(tx.account_sequence) => Some(tx.sender),
        DrcOperation::IssuedAssetPolicySet(tx) if selected(tx.account_sequence) => Some(tx.issuer),
        DrcOperation::TrustLineIssuerControl(tx) if selected(tx.account_sequence) => {
            Some(tx.issuer)
        }
        DrcOperation::IssuedClawback(tx) if selected(tx.account_sequence) => Some(tx.issuer),
        _ => None,
    }
}

fn journal_source_snapshots(
    journal: &UtxoJournal,
) -> impl Iterator<Item = &(Vec<u8>, Option<Vec<u8>>)> {
    journal
        .drc_policy_meta_before
        .iter()
        .chain(&journal.drc_deposit_preauth_meta_before)
        .chain(&journal.drc_regular_key_meta_before)
        .chain(&journal.drc_signer_list_meta_before)
        .chain(&journal.drc_ticket_meta_before)
        .chain(&journal.drc_escrow_meta_before)
        .chain(&journal.drc_check_meta_before)
        .chain(&journal.drc_payment_channel_meta_before)
        .chain(&journal.drc_trust_line_meta_before)
}

fn affected_object_keys(
    post: &StateStore,
    operations: &[DrcOperation],
    journal: &UtxoJournal,
) -> Result<BTreeSet<DrcLedgerObjectKey>, StateError> {
    let mut keys = BTreeSet::new();
    for operation in operations {
        keys.extend(direct_operation_object_keys(operation));
        if let Some(owner) = ticket_owner_if_consumed(operation) {
            keys.insert(DrcLedgerObjectKey::TicketSet { owner });
        }
    }
    for (source_key, prior) in journal_source_snapshots(journal) {
        let current = post.get_cf(ColumnFamily::Meta, source_key)?;
        let bytes = current.as_ref().or(prior.as_ref());
        if let Some(bytes) = bytes {
            if let Some(object) = decode_source_object(source_key, bytes)? {
                keys.insert(object.key());
            }
        }
    }
    Ok(keys)
}

fn snapshot_once(
    store: &StateStore,
    snapshots: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    key: Vec<u8>,
) -> Result<(), StateError> {
    if !snapshots.contains_key(&key) {
        snapshots.insert(key.clone(), store.get_cf(ColumnFamily::Meta, &key)?);
    }
    Ok(())
}

pub fn index_accepted_drc_operations_into(
    store: &StateStore,
    block: &Block,
    acceptance: &BlockAcceptanceRecord,
    application_blue_score: Option<u64>,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
) -> Result<(), StateError> {
    index_accepted_drc_operations_with_auth_into(
        store,
        block,
        acceptance,
        application_blue_score,
        None,
        batch,
        journal,
    )
}

pub fn index_accepted_drc_operations_with_auth_into(
    store: &StateStore,
    block: &Block,
    acceptance: &BlockAcceptanceRecord,
    application_blue_score: Option<u64>,
    auth: Option<&TxAuthContext>,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
) -> Result<(), StateError> {
    let schema = load_schema_version(store)?;
    let receipt_block = operation_receipt_block(block, auth)?;
    let operations = collect_accepted_operations(receipt_block.as_ref(), acceptance)?;
    if schema < DRC_LEDGER_INDEX_DATADIR_SCHEMA {
        // Schema-20 recovery must be able to finish an interrupted virtual
        // reorg before the canonical view can be migrated atomically. Older
        // schemas have no common index to maintain.
        return Ok(());
    }
    verify_drc_ledger_index_ready(store)?;
    let post = store.cow_overlay();
    post.write_batch(batch.clone())?;
    let object_keys = affected_object_keys(&post, &operations, journal)?;
    let marker = marker_bytes();
    let mut snapshots = BTreeMap::new();
    let mut index_batch = WriteBatch::new();

    for object_key in &object_keys {
        let object_id = object_key.object_id();
        let descriptor_key = object_by_id_key(&object_id);
        let prior_object = load_source_object(store, object_key)?;
        let prior_descriptor = load_descriptor_unchecked(store, &object_id)?;
        match (&prior_object, &prior_descriptor) {
            (Some(object), Some(descriptor))
                if descriptor == &DrcLedgerObjectDescriptor::new(object.clone()) => {}
            (None, None) => {}
            _ => {
                return Err(storage(
                    "DRC ledger-object mirror was inconsistent before block apply",
                ));
            }
        }
        snapshot_once(store, &mut snapshots, descriptor_key.clone())?;
        if let Some(descriptor) = &prior_descriptor {
            snapshot_once(
                store,
                &mut snapshots,
                object_by_owner_key(&descriptor.owner, descriptor.kind, &object_id),
            )?;
        }

        match load_source_object(&post, object_key)? {
            Some(object) => {
                let descriptor = DrcLedgerObjectDescriptor::new(object);
                let owner_key =
                    object_by_owner_key(&descriptor.owner, descriptor.kind, &descriptor.object_id);
                snapshot_once(store, &mut snapshots, owner_key.clone())?;
                index_batch.put_cf(
                    ColumnFamily::Meta,
                    &descriptor_key,
                    &borsh::to_vec(&descriptor).map_err(storage)?,
                );
                index_batch.put_cf(ColumnFamily::Meta, &owner_key, marker);
                if let Some(prior) = prior_descriptor {
                    let prior_owner =
                        object_by_owner_key(&prior.owner, prior.kind, &prior.object_id);
                    if prior_owner != owner_key {
                        index_batch.delete_cf(ColumnFamily::Meta, &prior_owner);
                    }
                }
            }
            None => {
                index_batch.delete_cf(ColumnFamily::Meta, &descriptor_key);
                if let Some(prior) = prior_descriptor {
                    index_batch.delete_cf(
                        ColumnFamily::Meta,
                        &object_by_owner_key(&prior.owner, prior.kind, &prior.object_id),
                    );
                }
            }
        }
    }

    let mut transaction_ids = BTreeSet::new();
    let all_affected_ids: BTreeSet<Hash> = object_keys
        .iter()
        .map(DrcLedgerObjectKey::object_id)
        .collect();
    for operation in operations {
        let operation_id = operation.operation_id();
        let transaction_id = operation.historical_transaction_id();
        if !transaction_ids.insert(transaction_id) {
            return Err(StateError::InvalidTx(
                "ambiguous historical DRC transaction ID in accepted block".into(),
            ));
        }
        let receipt_key = operation_by_id_key(&operation_id);
        let transaction_key = operation_by_tx_key(&transaction_id);
        if store.get_cf(ColumnFamily::Meta, &receipt_key)?.is_some()
            || store
                .get_cf(ColumnFamily::Meta, &transaction_key)?
                .is_some()
        {
            return Err(StateError::InvalidTx(
                "accepted DRC operation receipt replay".into(),
            ));
        }
        snapshot_once(store, &mut snapshots, receipt_key.clone())?;
        snapshot_once(store, &mut snapshots, transaction_key.clone())?;

        let mut affected: BTreeSet<Hash> = direct_operation_object_keys(&operation)
            .into_iter()
            .map(|key| key.object_id())
            .collect();
        if let Some(owner) = ticket_owner_if_consumed(&operation) {
            affected.insert(DrcLedgerObjectKey::TicketSet { owner }.object_id());
        }
        if let DrcOperation::IssuedAssetPolicySet(tx) = &operation {
            let asset = tx.asset_id();
            affected.extend(object_keys.iter().filter_map(|key| match key {
                DrcLedgerObjectKey::TrustLine {
                    asset: line_asset, ..
                } if *line_asset == asset => Some(key.object_id()),
                _ => None,
            }));
        }
        affected.retain(|id| all_affected_ids.contains(id));
        let receipt = DrcAcceptedOperationReceipt::new(
            block.id(),
            application_blue_score,
            operation,
            affected.into_iter().collect(),
        );
        index_batch.put_cf(
            ColumnFamily::Meta,
            &receipt_key,
            &borsh::to_vec(&receipt).map_err(storage)?,
        );
        index_batch.put_cf(
            ColumnFamily::Meta,
            &transaction_key,
            receipt.operation_id.as_bytes(),
        );
    }

    journal.drc_ledger_index_meta_before = snapshots.into_iter().collect();
    batch.append(index_batch);
    Ok(())
}

fn load_block_for_migration(store: &StateStore, block_id: &Hash) -> Result<Block, StateError> {
    for cf in [
        ColumnFamily::Hot,
        ColumnFamily::Archival,
        ColumnFamily::Warm,
    ] {
        if let Some(bytes) = store.get_cf(cf, block_id.as_bytes())? {
            if let Ok(block) = Block::try_from_slice(&bytes) {
                if block.id() != *block_id {
                    return Err(storage("migration block body ID mismatch"));
                }
                return Ok(block);
            }
        }
    }
    Err(storage(format!(
        "DRC ledger-object migration requires missing block body {}",
        block_id.to_hex()
    )))
}

fn put_descriptor_into(
    batch: &mut WriteBatch,
    descriptor: &DrcLedgerObjectDescriptor,
) -> Result<(), StateError> {
    batch.put_cf(
        ColumnFamily::Meta,
        &object_by_id_key(&descriptor.object_id),
        &borsh::to_vec(descriptor).map_err(storage)?,
    );
    batch.put_cf(
        ColumnFamily::Meta,
        &object_by_owner_key(&descriptor.owner, descriptor.kind, &descriptor.object_id),
        marker_bytes(),
    );
    Ok(())
}

fn receipt_affected_ids(
    operation: &DrcOperation,
    object_keys: &BTreeSet<DrcLedgerObjectKey>,
) -> Vec<Hash> {
    let all_affected_ids: BTreeSet<Hash> = object_keys
        .iter()
        .map(DrcLedgerObjectKey::object_id)
        .collect();
    let mut affected: BTreeSet<Hash> = direct_operation_object_keys(operation)
        .into_iter()
        .map(|key| key.object_id())
        .collect();
    if let Some(owner) = ticket_owner_if_consumed(operation) {
        affected.insert(DrcLedgerObjectKey::TicketSet { owner }.object_id());
    }
    if let DrcOperation::IssuedAssetPolicySet(tx) = operation {
        let asset = tx.asset_id();
        affected.extend(object_keys.iter().filter_map(|key| match key {
            DrcLedgerObjectKey::TrustLine {
                asset: line_asset, ..
            } if *line_asset == asset => Some(key.object_id()),
            _ => None,
        }));
    }
    affected.retain(|id| all_affected_ids.contains(id));
    affected.into_iter().collect()
}

/// Upgrade an exact schema-20 canonical view using its consensus blue apply order.
///
/// Every applied non-genesis block must retain its body, acceptance record, and
/// journal. The migration refuses partial index data and rewrites each historical
/// journal with the exact common-index snapshot needed for a later reorg.
pub fn migrate_drc_ledger_object_index_schema(
    store: &StateStore,
    applied_blocks: &[(Hash, u64)],
) -> Result<(), StateError> {
    migrate_drc_ledger_object_index_schema_with_auth(store, applied_blocks, None)
}

pub fn migrate_drc_ledger_object_index_schema_with_auth(
    store: &StateStore,
    applied_blocks: &[(Hash, u64)],
    auth: Option<&TxAuthContext>,
) -> Result<(), StateError> {
    let schema = load_schema_version(store)?;
    if schema == DRC_LEDGER_INDEX_DATADIR_SCHEMA {
        return verify_drc_ledger_object_index(store);
    }
    if schema != DRC_LEDGER_INDEX_DATADIR_SCHEMA - 1 {
        return Err(storage(format!(
            "DRC ledger-object migration requires schema {}, found {schema}",
            DRC_LEDGER_INDEX_DATADIR_SCHEMA - 1
        )));
    }
    if !store
        .scan_prefix_after_limit(ColumnFamily::Meta, common_index_prefix(), None, 1)?
        .is_empty()
    {
        return Err(storage(
            "schema 20 contains partial DRC ledger-object index data",
        ));
    }
    if applied_blocks.is_empty() {
        return Err(storage(
            "DRC ledger-object migration requires canonical applied order",
        ));
    }

    let ordered_ids: BTreeSet<Hash> = applied_blocks.iter().map(|(id, _)| *id).collect();
    if ordered_ids.len() != applied_blocks.len() {
        return Err(storage(
            "DRC ledger-object migration applied order contains duplicates",
        ));
    }
    for (key, _) in store.scan_prefix(ColumnFamily::Warm, b"utxo_diff/")? {
        let id = Hash(exact_suffix::<32>(&key, b"utxo_diff/")?);
        if !ordered_ids.contains(&id) {
            return Err(storage(format!(
                "applied journal {} is absent from migration order",
                id.to_hex()
            )));
        }
    }

    let mut records = Vec::with_capacity(applied_blocks.len().saturating_sub(1));
    for (position, (block_id, blue_score)) in applied_blocks.iter().enumerate() {
        let journal = crate::load_utxo_journal(store, block_id)?;
        if position == 0 && journal.is_none() {
            continue;
        }
        let journal = journal.ok_or_else(|| {
            storage(format!(
                "DRC ledger-object migration requires journal {}",
                block_id.to_hex()
            ))
        })?;
        if !journal.drc_ledger_index_meta_before.is_empty() {
            return Err(storage(
                "schema-20 journal unexpectedly contains DRC common-index snapshots",
            ));
        }
        let block = load_block_for_migration(store, block_id)?;
        let acceptance = crate::load_acceptance(store, block_id)?.ok_or_else(|| {
            storage(format!(
                "DRC ledger-object migration requires acceptance {}",
                block_id.to_hex()
            ))
        })?;
        if acceptance.block_hash != Hash::ZERO && acceptance.block_hash != *block_id {
            return Err(storage("migration acceptance block ID mismatch"));
        }
        records.push((*block_id, *blue_score, block, acceptance, journal));
    }

    let final_descriptors = collect_source_descriptors(store)?;
    let overlay = store.cow_overlay();
    let mut final_batch = WriteBatch::new();
    put_schema_version_into(&mut final_batch, DRC_LEDGER_INDEX_DATADIR_SCHEMA);
    for descriptor in final_descriptors.values() {
        put_descriptor_into(&mut final_batch, descriptor)?;
    }

    let mut transaction_ids = BTreeSet::new();
    let mut block_objects = BTreeMap::new();
    for (block_id, blue_score, block, acceptance, journal) in &records {
        let receipt_block = operation_receipt_block(block, auth)?;
        let operations = collect_accepted_operations(receipt_block.as_ref(), acceptance)?;
        let object_keys = affected_object_keys(store, &operations, journal)?;
        block_objects.insert(*block_id, object_keys.clone());
        for operation in operations {
            let transaction_id = operation.historical_transaction_id();
            if !transaction_ids.insert(transaction_id) {
                return Err(storage(
                    "historical DRC transaction ID is ambiguous during migration",
                ));
            }
            let affected_ids = receipt_affected_ids(&operation, &object_keys);
            let receipt = DrcAcceptedOperationReceipt::new(
                *block_id,
                Some(*blue_score),
                operation,
                affected_ids,
            );
            final_batch.put_cf(
                ColumnFamily::Meta,
                &operation_by_id_key(&receipt.operation_id),
                &borsh::to_vec(&receipt).map_err(storage)?,
            );
            final_batch.put_cf(
                ColumnFamily::Meta,
                &operation_by_tx_key(&receipt.historical_transaction_id),
                receipt.operation_id.as_bytes(),
            );
        }
    }
    overlay.write_batch(final_batch.clone())?;
    verify_drc_ledger_object_index(&overlay)?;

    for (block_id, _, block, acceptance, mut journal) in records.into_iter().rev() {
        let receipt_block = operation_receipt_block(&block, auth)?;
        let operations = collect_accepted_operations(receipt_block.as_ref(), &acceptance)?;
        let object_keys = block_objects
            .remove(&block_id)
            .ok_or_else(|| storage("migration lost block object set"))?;
        let mut after_descriptors = BTreeMap::new();
        for object_key in &object_keys {
            let object_id = object_key.object_id();
            after_descriptors.insert(object_id, load_descriptor_unchecked(&overlay, &object_id)?);
        }

        let source_revert = crate::revert_journal_batched(&journal)?;
        overlay.write_batch(source_revert)?;

        let mut snapshots = BTreeMap::new();
        let mut reverse_index = WriteBatch::new();
        for object_key in &object_keys {
            let object_id = object_key.object_id();
            let descriptor_key = object_by_id_key(&object_id);
            let before_descriptor =
                load_source_object(&overlay, object_key)?.map(DrcLedgerObjectDescriptor::new);
            snapshots.insert(
                descriptor_key.clone(),
                before_descriptor
                    .as_ref()
                    .map(|descriptor| borsh::to_vec(descriptor).map_err(storage))
                    .transpose()?,
            );

            let after_owner = after_descriptors
                .get(&object_id)
                .and_then(Option::as_ref)
                .map(|descriptor| {
                    object_by_owner_key(&descriptor.owner, descriptor.kind, &object_id)
                });
            let before_owner = before_descriptor.as_ref().map(|descriptor| {
                object_by_owner_key(&descriptor.owner, descriptor.kind, &object_id)
            });
            if let Some(key) = &after_owner {
                snapshots.insert(
                    key.clone(),
                    if before_owner.as_ref() == Some(key) {
                        Some(marker_bytes().to_vec())
                    } else {
                        None
                    },
                );
                reverse_index.delete_cf(ColumnFamily::Meta, key);
            }
            if let Some(key) = &before_owner {
                snapshots.insert(key.clone(), Some(marker_bytes().to_vec()));
                reverse_index.put_cf(ColumnFamily::Meta, key, marker_bytes());
            }
            match before_descriptor {
                Some(descriptor) => put_descriptor_into(&mut reverse_index, &descriptor)?,
                None => reverse_index.delete_cf(ColumnFamily::Meta, &descriptor_key),
            }
        }
        for operation in operations {
            let receipt_key = operation_by_id_key(&operation.operation_id());
            let transaction_key = operation_by_tx_key(&operation.historical_transaction_id());
            if overlay.get_cf(ColumnFamily::Meta, &receipt_key)?.is_none()
                || overlay
                    .get_cf(ColumnFamily::Meta, &transaction_key)?
                    .is_none()
            {
                return Err(storage(
                    "migration receipt history does not match applied order",
                ));
            }
            snapshots.insert(receipt_key.clone(), None);
            snapshots.insert(transaction_key.clone(), None);
            reverse_index.delete_cf(ColumnFamily::Meta, &receipt_key);
            reverse_index.delete_cf(ColumnFamily::Meta, &transaction_key);
        }
        overlay.write_batch(reverse_index)?;
        journal.drc_ledger_index_meta_before = snapshots.into_iter().collect();
        final_batch.put_cf(
            ColumnFamily::Warm,
            &crate::utxo_diff_key(&block_id),
            &borsh::to_vec(&journal).map_err(storage)?,
        );
    }
    verify_drc_ledger_object_index(&overlay)?;
    store.write_batch(final_batch)?;
    verify_drc_ledger_object_index(store)
}

/// Atomically rebuild the derivable live-object mirror while preserving verified
/// accepted-operation history.
pub fn reindex_drc_ledger_objects(store: &StateStore) -> Result<(), StateError> {
    verify_drc_ledger_index_ready(store)?;
    let descriptors = collect_source_descriptors(store)?;
    let mut batch = WriteBatch::new();
    for (key, _) in store.scan_prefix(ColumnFamily::Meta, OBJECT_BY_ID_PREFIX)? {
        batch.delete_cf(ColumnFamily::Meta, &key);
    }
    for (key, _) in store.scan_prefix(ColumnFamily::Meta, OBJECT_BY_OWNER_PREFIX)? {
        batch.delete_cf(ColumnFamily::Meta, &key);
    }
    let marker = marker_bytes();
    for descriptor in descriptors.values() {
        batch.put_cf(
            ColumnFamily::Meta,
            &object_by_id_key(&descriptor.object_id),
            &borsh::to_vec(descriptor).map_err(storage)?,
        );
        batch.put_cf(
            ColumnFamily::Meta,
            &object_by_owner_key(&descriptor.owner, descriptor.kind, &descriptor.object_id),
            marker,
        );
    }
    let overlay = store.cow_overlay();
    overlay.write_batch(batch.clone())?;
    verify_drc_ledger_object_index(&overlay)?;
    store.write_batch(batch)
}

pub(crate) fn common_index_prefix() -> &'static [u8] {
    INDEX_PREFIX
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::SCHEMA_VERSION;
    use crate::drc_escrow_test_harness::support::{
        apply_block_capture, auth, coinbase, fund, key, signed_create,
    };
    use crate::supply::{put_burned_supply_into, put_schema_version_into};
    use agora_types::{
        Amount, DrcAccountSignerList, DrcAccountTickets, DrcCheckLive, DrcEscrowLive,
        DrcIssuedAssetPolicyLive, DrcLedgerObject, DrcPaymentChannelLive, DrcSignerListEntry,
        DrcTrustLineLive, IssuedAmount, IssuedAssetId, IssuedCurrencyCode,
        DRC_CHECK_LIVE_STATE_VERSION, DRC_ESCROW_LIVE_STATE_VERSION,
        DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION, DRC_SIGNER_LIST_STATE_VERSION,
        DRC_TICKET_STATE_VERSION,
    };

    fn ready_store() -> StateStore {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        for asset in NativeAssetId::ALL {
            put_burned_supply_into(&mut batch, asset, 0);
        }
        put_schema_version_into(&mut batch, SCHEMA_VERSION);
        initialize_drc_ledger_index_into(&mut batch);
        store.write_batch(batch).unwrap();
        store
    }

    #[test]
    fn empty_index_verifies_and_has_stable_root() {
        let store = ready_store();
        verify_drc_ledger_object_index(&store).unwrap();
        assert_eq!(
            drc_ledger_object_index_root(&store).unwrap(),
            drc_ledger_object_index_root(&store).unwrap()
        );
    }

    #[test]
    fn pagination_rejects_malformed_and_foreign_cursors() {
        let store = ready_store();
        let owner = Address([1; 20]);
        assert!(list_drc_account_objects(&store, owner, None, 0, None).is_err());
        assert!(list_drc_account_objects(&store, owner, None, 101, None).is_err());
        assert!(list_drc_account_objects(&store, owner, None, 1, Some("xyz")).is_err());
        let cursor = encode_cursor(&OwnerCursor::new(
            Address([2; 20]),
            None,
            DrcLedgerObjectKind::TicketSet,
            Hash([3; 32]),
        ))
        .unwrap();
        assert!(list_drc_account_objects(&store, owner, None, 1, Some(&cursor)).is_err());

        let cursor = encode_cursor(&OwnerCursor::new(
            owner,
            None,
            DrcLedgerObjectKind::TicketSet,
            Hash([3; 32]),
        ))
        .unwrap();
        assert!(list_drc_account_objects(
            &store,
            owner,
            Some(DrcLedgerObjectKind::TicketSet),
            1,
            Some(&cursor),
        )
        .is_err());

        let mut tampered = hex::decode(cursor).unwrap();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(
            list_drc_account_objects(&store, owner, None, 1, Some(&hex::encode(tampered)),)
                .is_err()
        );
    }

    #[test]
    fn missing_marker_fails_closed() {
        let store = StateStore::open_in_memory();
        store
            .put_cf(
                ColumnFamily::Meta,
                crate::meta_keys::SCHEMA_VERSION,
                &SCHEMA_VERSION.to_le_bytes(),
            )
            .unwrap();
        assert!(verify_drc_ledger_object_index(&store).is_err());
    }

    fn put_preauth(store: &StateStore, owner: Address, source: Address) {
        let record = DrcDepositPreauth::new(owner, source);
        store
            .put_cf(
                ColumnFamily::Meta,
                &drc_deposit_preauth_key(&owner, &source),
                &borsh::to_vec(&record).unwrap(),
            )
            .unwrap();
    }

    fn put_source_object(store: &StateStore, object: &DrcLedgerObject) {
        let bytes = match object {
            DrcLedgerObject::AccountPolicy { policy, .. } => borsh::to_vec(policy),
            DrcLedgerObject::DepositPreauthorization(value) => borsh::to_vec(value),
            DrcLedgerObject::RegularKey(value) => borsh::to_vec(value),
            DrcLedgerObject::SignerList(value) => borsh::to_vec(value),
            DrcLedgerObject::TicketSet(value) => borsh::to_vec(value),
            DrcLedgerObject::Escrow(value) => borsh::to_vec(value),
            DrcLedgerObject::Check(value) => borsh::to_vec(value),
            DrcLedgerObject::PaymentChannel(value) => borsh::to_vec(value),
            DrcLedgerObject::TrustLine(value) => borsh::to_vec(value),
            DrcLedgerObject::IssuedAssetPolicy(value) => borsh::to_vec(value),
        }
        .unwrap();
        store
            .put_cf(ColumnFamily::Meta, &source_key(&object.key()), &bytes)
            .unwrap();
    }

    fn all_sample_objects() -> Vec<DrcLedgerObject> {
        let owner = Address([0x11; 20]);
        let peer = Address([0x22; 20]);
        let issuer = Address([0x33; 20]);
        let mut currency = [0u8; 20];
        currency[..3].copy_from_slice(b"USD");
        let asset = IssuedAssetId {
            issuer,
            currency: IssuedCurrencyCode(currency),
        };
        vec![
            DrcLedgerObject::AccountPolicy {
                account: owner,
                policy: DrcAccountPolicy::default(),
            },
            DrcLedgerObject::DepositPreauthorization(DrcDepositPreauth::new(owner, peer)),
            DrcLedgerObject::RegularKey(DrcAccountRegularKey::new(owner, peer)),
            DrcLedgerObject::SignerList(DrcAccountSignerList {
                version: DRC_SIGNER_LIST_STATE_VERSION,
                owner,
                quorum: 1,
                entries: vec![DrcSignerListEntry {
                    signer: peer,
                    weight: 1,
                }],
            }),
            DrcLedgerObject::TicketSet(DrcAccountTickets {
                version: DRC_TICKET_STATE_VERSION,
                owner,
                sequences: vec![7, 8],
            }),
            DrcLedgerObject::Escrow(DrcEscrowLive {
                version: DRC_ESCROW_LIVE_STATE_VERSION,
                escrow_id: Hash([0x61; 32]),
                owner,
                recipient: peer,
                amount: Amount::from_base_units(10),
                destination_tag: None,
                source_tag: None,
                invoice_id: Hash::ZERO,
                finish_after_blue_score: None,
                cancel_after_blue_score: Some(100),
                create_blue_score: 1,
            }),
            DrcLedgerObject::Check(DrcCheckLive {
                version: DRC_CHECK_LIVE_STATE_VERSION,
                check_id: Hash([0x62; 32]),
                owner,
                destination: peer,
                amount: Amount::from_base_units(11),
                destination_tag: None,
                source_tag: None,
                invoice_id: Hash::ZERO,
                expires_after_blue_score: Some(100),
                create_blue_score: 1,
            }),
            DrcLedgerObject::PaymentChannel(DrcPaymentChannelLive {
                version: DRC_PAYMENT_CHANNEL_LIVE_STATE_VERSION,
                channel_id: Hash([0x63; 32]),
                owner,
                destination: peer,
                destination_tag: None,
                source_tag: None,
                invoice_id: Hash::ZERO,
                claim_public_key: vec![2; 33],
                settle_delay_blue_scores: 2,
                cancel_after_blue_score: Some(100),
                total_funded: Amount::from_base_units(20),
                cumulative_claimed: Amount::from_base_units(3),
                close_finalizable_after: None,
                create_blue_score: 1,
            }),
            DrcLedgerObject::TrustLine(DrcTrustLineLive::v1_defaults(
                owner,
                asset,
                IssuedAmount::from_units(100),
                IssuedAmount::from_units(4),
                false,
            )),
            DrcLedgerObject::IssuedAssetPolicy(DrcIssuedAssetPolicyLive::default_for_asset(asset)),
        ]
    }

    #[test]
    fn all_supported_live_families_have_typed_ids_point_lookup_and_owner_rows() {
        let store = ready_store();
        let objects = all_sample_objects();
        for object in &objects {
            put_source_object(&store, object);
        }
        reindex_drc_ledger_objects(&store).unwrap();

        let mut kinds = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for object in objects {
            let expected = DrcLedgerObjectDescriptor::new(object);
            let loaded = load_drc_ledger_object(&store, &expected.object_id)
                .unwrap()
                .expect("sample object is indexed");
            assert_eq!(loaded, expected);
            assert!(kinds.insert(loaded.kind));
            assert!(ids.insert(loaded.object_id));
            let owner_page =
                list_drc_account_objects(&store, loaded.owner, Some(loaded.kind), 100, None)
                    .unwrap();
            assert!(owner_page
                .objects
                .iter()
                .any(|candidate| candidate.object_id == loaded.object_id));
        }
        assert_eq!(kinds, DrcLedgerObjectKind::ALL.into_iter().collect());
        assert_eq!(ids.len(), DrcLedgerObjectKind::ALL.len());
        verify_drc_ledger_object_index(&store).unwrap();
    }

    #[test]
    fn bounded_pagination_is_stable_filtered_and_owner_isolated() {
        let store = ready_store();
        let owner = Address([1; 20]);
        let other = Address([2; 20]);
        for byte in [3, 4, 5] {
            put_preauth(&store, owner, Address([byte; 20]));
        }
        put_preauth(&store, other, Address([6; 20]));
        reindex_drc_ledger_objects(&store).unwrap();

        let first = list_drc_account_objects(&store, owner, None, 2, None).unwrap();
        assert_eq!(first.objects.len(), 2);
        assert!(first
            .objects
            .iter()
            .all(|descriptor| descriptor.owner == owner));
        let second =
            list_drc_account_objects(&store, owner, None, 2, first.next_cursor.as_deref()).unwrap();
        assert_eq!(second.objects.len(), 1);
        assert!(second.next_cursor.is_none());
        let mut paged: Vec<Hash> = first
            .objects
            .into_iter()
            .chain(second.objects)
            .map(|descriptor| descriptor.object_id)
            .collect();
        let mut sorted = paged.clone();
        sorted.sort();
        assert_eq!(paged, sorted);
        paged.dedup();
        assert_eq!(paged.len(), 3);

        let filtered = list_drc_account_objects(
            &store,
            owner,
            Some(DrcLedgerObjectKind::DepositPreauthorization),
            100,
            None,
        )
        .unwrap();
        assert_eq!(filtered.objects.len(), 3);
        assert!(list_drc_account_objects(
            &store,
            other,
            Some(DrcLedgerObjectKind::Escrow),
            100,
            None,
        )
        .unwrap()
        .objects
        .is_empty());

        let foreign_cursor = encode_cursor(&OwnerCursor::new(
            other,
            None,
            DrcLedgerObjectKind::DepositPreauthorization,
            Hash([9; 32]),
        ))
        .unwrap();
        assert!(list_drc_account_objects(&store, owner, None, 1, Some(&foreign_cursor)).is_err());
    }

    #[test]
    fn root_and_ids_are_independent_of_source_insertion_order() {
        let build = |reverse: bool| {
            let store = ready_store();
            let owner = Address([7; 20]);
            let mut sources = vec![Address([8; 20]), Address([9; 20])];
            if reverse {
                sources.reverse();
            }
            for source in sources {
                put_preauth(&store, owner, source);
            }
            reindex_drc_ledger_objects(&store).unwrap();
            let page = list_drc_account_objects(&store, owner, None, 100, None).unwrap();
            (
                drc_ledger_object_index_root(&store).unwrap(),
                page.objects
                    .into_iter()
                    .map(|descriptor| descriptor.object_id)
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(build(false), build(true));
    }

    #[test]
    fn point_lookup_and_global_verify_fail_on_owner_index_corruption() {
        let store = ready_store();
        let owner = Address([10; 20]);
        put_preauth(&store, owner, Address([11; 20]));
        reindex_drc_ledger_objects(&store).unwrap();
        let descriptor = list_drc_account_objects(&store, owner, None, 1, None)
            .unwrap()
            .objects
            .remove(0);
        store
            .delete_cf(
                ColumnFamily::Meta,
                &object_by_owner_key(&descriptor.owner, descriptor.kind, &descriptor.object_id),
            )
            .unwrap();
        assert!(load_drc_ledger_object(&store, &descriptor.object_id).is_err());
        assert!(verify_drc_ledger_object_index(&store).is_err());
    }

    #[test]
    fn missing_receipt_fails_closed_and_reindex_does_not_partially_commit() {
        let store = ready_store();
        let owner = key(12);
        let recipient = key(13);
        let ctx = auth();
        fund(&store, &owner, 1_000);
        let create = signed_create(
            &owner,
            recipient.address(),
            10,
            Some(1),
            None,
            0,
            &ctx,
            None,
        );
        let operation = DrcOperation::EscrowCreate(create.clone());
        let object_id = DrcLedgerObjectKey::Escrow {
            escrow_id: create.escrow_id(),
        }
        .object_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_block_capture(&store, block, 1, &ctx);

        let descriptor = load_drc_ledger_object(&store, &object_id).unwrap().unwrap();
        let owner_key =
            object_by_owner_key(&descriptor.owner, descriptor.kind, &descriptor.object_id);
        store
            .delete_cf(
                ColumnFamily::Meta,
                &operation_by_id_key(&operation.operation_id()),
            )
            .unwrap();
        store.delete_cf(ColumnFamily::Meta, &owner_key).unwrap();

        assert!(load_drc_transaction(&store, &operation.historical_transaction_id()).is_err());
        assert!(verify_drc_ledger_object_index(&store).is_err());
        assert!(reindex_drc_ledger_objects(&store).is_err());
        assert!(
            store
                .get_cf(ColumnFamily::Meta, &owner_key)
                .unwrap()
                .is_none(),
            "failed reindex must not commit a partial object repair"
        );
    }

    #[test]
    fn accepted_create_revert_and_reapply_tracks_object_and_receipt_atomically() {
        let store = ready_store();
        let owner = key(21);
        let recipient = key(22);
        let ctx = auth();
        fund(&store, &owner, 1_000);
        let create = signed_create(
            &owner,
            recipient.address(),
            25,
            Some(1),
            None,
            0,
            &ctx,
            None,
        );
        let escrow_id = create.escrow_id();
        let operation = DrcOperation::EscrowCreate(create.clone());
        let object_id = DrcLedgerObjectKey::Escrow { escrow_id }.object_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        let (block_id, journal, _) = apply_block_capture(&store, block.clone(), 1, &ctx);
        let root = drc_ledger_object_index_root(&store).unwrap();

        assert!(load_drc_ledger_object(&store, &object_id)
            .unwrap()
            .is_some());
        assert_eq!(
            load_drc_transaction(&store, &operation.historical_transaction_id())
                .unwrap()
                .unwrap()
                .operation_id,
            operation.operation_id()
        );
        assert_eq!(
            load_drc_operation(&store, &operation.operation_id())
                .unwrap()
                .unwrap()
                .canonical_block_id,
            block_id
        );

        store
            .write_batch(crate::revert_journal_batched(&journal).unwrap())
            .unwrap();
        assert!(load_drc_ledger_object(&store, &object_id)
            .unwrap()
            .is_none());
        assert!(load_drc_operation(&store, &operation.operation_id())
            .unwrap()
            .is_none());

        let (_, reapplied, _) = apply_block_capture(&store, block, 1, &ctx);
        assert_eq!(drc_ledger_object_index_root(&store).unwrap(), root);
        assert_eq!(
            reapplied.drc_ledger_index_meta_before,
            journal.drc_ledger_index_meta_before
        );
    }

    #[test]
    fn competing_branch_revert_removes_orphan_and_accepts_replacement() {
        let store = ready_store();
        let owner = key(31);
        let first_recipient = key(32);
        let second_recipient = key(33);
        let ctx = auth();
        fund(&store, &owner, 1_000);

        let first = signed_create(
            &owner,
            first_recipient.address(),
            10,
            Some(1),
            None,
            0,
            &ctx,
            None,
        );
        let first_operation = DrcOperation::EscrowCreate(first.clone());
        let mut first_block = coinbase(vec![Hash::ZERO], &owner);
        first_block.drc_escrow_creates.push(first);
        let (_, first_journal, _) = apply_block_capture(&store, first_block, 1, &ctx);
        store
            .write_batch(crate::revert_journal_batched(&first_journal).unwrap())
            .unwrap();

        let second = signed_create(
            &owner,
            second_recipient.address(),
            11,
            Some(1),
            None,
            0,
            &ctx,
            None,
        );
        let second_operation = DrcOperation::EscrowCreate(second.clone());
        let mut second_block = coinbase(vec![Hash::ZERO], &owner);
        second_block.drc_escrow_creates.push(second);
        apply_block_capture(&store, second_block, 1, &ctx);

        assert!(load_drc_operation(&store, &first_operation.operation_id())
            .unwrap()
            .is_none());
        assert!(load_drc_operation(&store, &second_operation.operation_id())
            .unwrap()
            .is_some());
        let page = list_drc_account_objects(&store, owner.address(), None, 100, None).unwrap();
        assert_eq!(
            page.objects
                .iter()
                .filter(|object| object.kind == DrcLedgerObjectKind::Escrow)
                .count(),
            1
        );
    }

    #[test]
    fn schema20_migration_rebuilds_history_and_reorg_snapshots() {
        let store = ready_store();
        let owner = key(41);
        let recipient = key(42);
        let ctx = auth();
        fund(&store, &owner, 1_000);
        let create = signed_create(
            &owner,
            recipient.address(),
            10,
            Some(1),
            None,
            0,
            &ctx,
            None,
        );
        let operation = DrcOperation::EscrowCreate(create.clone());
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        block.header.tx_root = block.compute_body_root();

        let mut downgrade = WriteBatch::new();
        for (key, _) in store
            .scan_prefix(ColumnFamily::Meta, common_index_prefix())
            .unwrap()
        {
            downgrade.delete_cf(ColumnFamily::Meta, &key);
        }
        downgrade.put_cf(
            ColumnFamily::Meta,
            crate::meta_keys::SCHEMA_VERSION,
            &(DRC_LEDGER_INDEX_DATADIR_SCHEMA - 1).to_le_bytes(),
        );
        store.write_batch(downgrade).unwrap();

        let (block_id, journal, acceptance) = apply_block_capture(&store, block.clone(), 1, &ctx);
        assert!(journal.drc_ledger_index_meta_before.is_empty());
        assert!(store
            .scan_prefix(ColumnFamily::Meta, common_index_prefix())
            .unwrap()
            .is_empty());
        store
            .put_cf(
                ColumnFamily::Hot,
                block_id.as_bytes(),
                &borsh::to_vec(&block).unwrap(),
            )
            .unwrap();
        crate::store_utxo_journal(&store, &block_id, &journal).unwrap();
        crate::store_acceptance(&store, &block_id, &acceptance).unwrap();

        migrate_drc_ledger_object_index_schema(&store, &[(Hash::ZERO, 0), (block_id, 1)]).unwrap();
        assert!(load_drc_operation(&store, &operation.operation_id())
            .unwrap()
            .is_some());
        let migrated = crate::load_utxo_journal(&store, &block_id)
            .unwrap()
            .unwrap();
        assert!(!migrated.drc_ledger_index_meta_before.is_empty());
        store
            .write_batch(crate::revert_journal_batched(&migrated).unwrap())
            .unwrap();
        assert!(load_drc_operation(&store, &operation.operation_id())
            .unwrap()
            .is_none());
        verify_drc_ledger_object_index(&store).unwrap();
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn index_survives_rocksdb_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let owner = Address([51; 20]);
        let object_id = {
            let store = StateStore::open(directory.path()).unwrap();
            let mut batch = WriteBatch::new();
            put_schema_version_into(&mut batch, SCHEMA_VERSION);
            store.write_batch(batch).unwrap();
            put_preauth(&store, owner, Address([52; 20]));
            reindex_drc_ledger_objects(&store).unwrap();
            list_drc_account_objects(&store, owner, None, 1, None)
                .unwrap()
                .objects[0]
                .object_id
        };
        let reopened = StateStore::open(directory.path()).unwrap();
        assert!(load_drc_ledger_object(&reopened, &object_id)
            .unwrap()
            .is_some());
        verify_drc_ledger_object_index(&reopened).unwrap();
    }
}
