//! DRC weighted signer-list state and rotation transitions.

use agora_types::{
    canonical_sorted_entries, Address, DrcAccountSignerList, DrcSignerListAction, DrcSignerListTx,
    Hash, NativeAssetId,
};
use borsh::BorshDeserialize;

use crate::accounts::{account_exists, load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_account_auth::verify_drc_signer_list_operation;
use crate::drc_master_key_recovery::ensure_no_lockout_after_signer_list;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const DRC_SIGNER_LIST_PREFIX: &[u8] = b"account/drc/signer-list/";
pub const DRC_SIGNER_LIST_ROOT_DOMAIN: &[u8] = b"agora-drc-signer-list-root-v1";

pub fn drc_signer_list_meta_key(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_SIGNER_LIST_PREFIX.len() + owner.0.len());
    key.extend_from_slice(DRC_SIGNER_LIST_PREFIX);
    key.extend_from_slice(&owner.0);
    key
}

pub fn drc_signer_list_meta_keys(tx: &DrcSignerListTx) -> Vec<Vec<u8>> {
    vec![drc_signer_list_meta_key(&tx.owner)]
}

pub fn load_drc_account_signer_list(
    store: &StateStore,
    owner: &Address,
) -> Result<Option<DrcAccountSignerList>, StateError> {
    let key = drc_signer_list_meta_key(owner);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(None);
    };
    let record = DrcAccountSignerList::try_from_slice(&bytes)
        .map_err(|error| StateError::Storage(error.to_string()))?;
    record
        .validate()
        .map_err(|error| StateError::Storage(error.to_string()))?;
    if record.owner != *owner {
        return Err(StateError::Storage(
            "DRC signer-list record does not match index key".into(),
        ));
    }
    Ok(Some(record))
}

pub fn load_known_drc_account_signer_summary(
    store: &StateStore,
    owner: &Address,
) -> Result<Option<(u32, u32, u64)>, StateError> {
    if !account_exists(store, NativeAssetId::DRC, owner)? {
        return Ok(None);
    }
    let Some(list) = load_drc_account_signer_list(store, owner)? else {
        let account = load_account(store, NativeAssetId::DRC, owner)?;
        return Ok(Some((0, 0, account.nonce)));
    };
    let account = load_account(store, NativeAssetId::DRC, owner)?;
    Ok(Some((
        list.quorum,
        list.entries.len() as u32,
        account.nonce,
    )))
}

pub fn drc_signer_list_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, DRC_SIGNER_LIST_PREFIX)? {
        if key.len() != DRC_SIGNER_LIST_PREFIX.len() + 20 {
            return Err(StateError::Storage(
                "invalid DRC signer-list key length".into(),
            ));
        }
        let mut owner = [0; 20];
        owner.copy_from_slice(&key[DRC_SIGNER_LIST_PREFIX.len()..]);
        let record = DrcAccountSignerList::try_from_slice(&bytes)
            .map_err(|error| StateError::Storage(error.to_string()))?;
        record
            .validate()
            .map_err(|error| StateError::Storage(error.to_string()))?;
        if record.owner != Address(owner) {
            return Err(StateError::Storage(
                "DRC signer-list record does not match index key".into(),
            ));
        }
        entries.push(record);
    }
    entries.sort_by_key(|record| record.owner.0);
    Ok(Hash::hash_borsh(&(DRC_SIGNER_LIST_ROOT_DOMAIN, entries)))
}

pub fn apply_drc_signer_list(
    store: &StateStore,
    tx: &DrcSignerListTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<Option<DrcAccountSignerList>, StateError> {
    verify_drc_signer_list_operation(store, tx, auth)?;
    ensure_no_lockout_after_signer_list(store, tx)?;

    if !account_exists(store, NativeAssetId::DRC, &tx.owner)? {
        return Err(StateError::InvalidTx(
            "unknown DRC signer-list owner".into(),
        ));
    }

    let key = drc_signer_list_meta_key(&tx.owner);
    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    if owner.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad DRC signer-list nonce: got {} expected {}",
            tx.nonce, owner.nonce
        )));
    }
    if owner.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC signer-list balance".into(),
        ));
    }
    let next_nonce = owner
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC signer-list nonce overflow".into()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.owner, owner.clone()));
    owner.balance -= tx.fee.as_base_units();
    owner.nonce = next_nonce;
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;

    match tx.action {
        DrcSignerListAction::Set => {
            let entries = canonical_sorted_entries(&tx.entries);
            let record = DrcAccountSignerList {
                version: agora_types::DRC_SIGNER_LIST_STATE_VERSION,
                owner: tx.owner,
                quorum: tx.quorum,
                entries,
            };
            record
                .validate()
                .map_err(|error| StateError::InvalidTx(error.to_string()))?;
            let bytes =
                borsh::to_vec(&record).map_err(|error| StateError::Storage(error.to_string()))?;
            batch.put_cf(ColumnFamily::Meta, &key, &bytes);
            Ok(Some(record))
        }
        DrcSignerListAction::Delete => {
            batch.delete_cf(ColumnFamily::Meta, &key);
            Ok(None)
        }
    }
}
