//! DRC regular-key state and rotation transitions.

use agora_types::{
    Address, DrcAccountRegularKey, DrcRegularKeyAction, DrcRegularKeyTx, Hash, NativeAssetId,
};
use borsh::BorshDeserialize;

use crate::accounts::{account_exists, load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::verify_drc_regular_key_operation;
use crate::drc_master_key_recovery::ensure_no_lockout_after_regular_key;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const DRC_REGULAR_KEY_PREFIX: &[u8] = b"account/drc/regular-key/";
pub const DRC_REGULAR_KEY_ROOT_DOMAIN: &[u8] = b"agora-drc-regular-key-root-v1";

pub fn drc_regular_key_meta_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_REGULAR_KEY_PREFIX.len() + owner.0.len());
    key.extend_from_slice(DRC_REGULAR_KEY_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn drc_regular_key_meta_keys(tx: &DrcRegularKeyTx) -> Vec<Vec<u8>> {
    vec![drc_regular_key_meta_key(&tx.owner)]
}

/// Absence means master-only authorization.
pub fn load_drc_account_regular_key(
    store: &StateStore,
    owner: &Address,
) -> Result<Option<Address>, StateError> {
    let key = drc_regular_key_meta_key(owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(None);
    };
    let record = DrcAccountRegularKey::try_from_slice(&bytes)
        .map_err(|error| StateError::Storage(error.to_string()))?;
    record
        .validate()
        .map_err(|error| StateError::Storage(error.to_string()))?;
    if record.owner != *owner {
        return Err(StateError::Storage(
            "DRC regular-key record does not match index key".into(),
        ));
    }
    Ok(Some(record.regular_key))
}

pub fn load_known_drc_account_keys(
    store: &StateStore,
    owner: &Address,
) -> Result<Option<(Option<Address>, u64)>, StateError> {
    if !account_exists(store, NativeAssetId::DRC, owner)? {
        return Ok(None);
    }
    let regular_key = load_drc_account_regular_key(store, owner)?;
    let account = load_account(store, NativeAssetId::DRC, owner)?;
    Ok(Some((regular_key, account.nonce)))
}

pub fn drc_regular_key_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, DRC_REGULAR_KEY_PREFIX)? {
        if key.len() != DRC_REGULAR_KEY_PREFIX.len() + 20 {
            return Err(StateError::Storage(
                "invalid DRC regular-key key length".into(),
            ));
        }
        let mut owner = [0; 20];
        owner.copy_from_slice(&key[DRC_REGULAR_KEY_PREFIX.len()..]);
        let record = DrcAccountRegularKey::try_from_slice(&bytes)
            .map_err(|error| StateError::Storage(error.to_string()))?;
        record
            .validate()
            .map_err(|error| StateError::Storage(error.to_string()))?;
        if record.owner != Address(owner) {
            return Err(StateError::Storage(
                "DRC regular-key record does not match index key".into(),
            ));
        }
        entries.push(record);
    }
    entries.sort_by_key(|record| record.owner.0);
    Ok(Hash::hash_borsh(&(DRC_REGULAR_KEY_ROOT_DOMAIN, entries)))
}

pub fn apply_drc_regular_key(
    store: &StateStore,
    tx: &DrcRegularKeyTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<Option<Address>, StateError> {
    verify_drc_regular_key_operation(store, tx, auth)?;
    ensure_no_lockout_after_regular_key(store, tx)?;

    if !account_exists(store, NativeAssetId::DRC, &tx.owner)? {
        return Err(StateError::InvalidTx(
            "unknown DRC regular-key owner".into(),
        ));
    }

    let key = drc_regular_key_meta_key(&tx.owner);
    let current = load_drc_account_regular_key(store, &tx.owner)?;
    match tx.action {
        DrcRegularKeyAction::Set => {
            if current == Some(tx.regular_key) {
                // Idempotent reinstall still consumes nonce/fee.
            }
        }
        DrcRegularKeyAction::Clear => {
            if current.is_none() {
                // Idempotent clear.
            }
        }
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    if owner.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad DRC regular-key nonce: got {} expected {}",
            tx.nonce, owner.nonce
        )));
    }
    if owner.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC regular-key balance".into(),
        ));
    }
    let next_nonce = owner
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC regular-key nonce overflow".into()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.owner, owner.clone()));
    owner.balance -= tx.fee.as_base_units();
    owner.nonce = next_nonce;
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;

    match tx.action {
        DrcRegularKeyAction::Set => {
            let record = DrcAccountRegularKey::new(tx.owner, tx.regular_key);
            let bytes =
                borsh::to_vec(&record).map_err(|error| StateError::Storage(error.to_string()))?;
            batch.put_cf(ColumnFamily::Meta, &key, &bytes);
            Ok(Some(tx.regular_key))
        }
        DrcRegularKeyAction::Clear => {
            batch.delete_cf(ColumnFamily::Meta, &key);
            Ok(None)
        }
    }
}
