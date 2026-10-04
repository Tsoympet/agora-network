//! DRC master-key disable recovery paths and no-lockout guards.

use agora_types::{
    Address, DrcAccountSignerList, DrcRegularKeyAction, DrcRegularKeyTx, DrcSignerListAction,
    DrcSignerListTx,
};

use crate::drc_policy::load_drc_account_policy;
use crate::drc_regular_key::load_drc_account_regular_key;
use crate::drc_signer_list::load_drc_account_signer_list;
use crate::{StateError, StateStore};

pub fn drc_master_key_disabled(store: &StateStore, owner: &Address) -> Result<bool, StateError> {
    Ok(load_drc_account_policy(store, owner)?.master_key_disabled)
}

pub fn has_usable_regular_key(store: &StateStore, owner: &Address) -> Result<bool, StateError> {
    Ok(load_drc_account_regular_key(store, owner)?.is_some())
}

pub fn has_usable_signer_list(store: &StateStore, owner: &Address) -> Result<bool, StateError> {
    Ok(load_drc_account_signer_list(store, owner)?.is_some())
}

/// At least one live alternate recovery path (regular key or signer list).
pub fn has_alternate_recovery(store: &StateStore, owner: &Address) -> Result<bool, StateError> {
    Ok(has_usable_regular_key(store, owner)? || has_usable_signer_list(store, owner)?)
}

fn ensure_alternate_recovery(
    regular_key: Option<Address>,
    signer_list: Option<&DrcAccountSignerList>,
) -> Result<(), StateError> {
    if regular_key.is_some() || signer_list.is_some() {
        Ok(())
    } else {
        Err(StateError::InvalidTx(
            "DRC master-key disable requires a live regular key or signer list".into(),
        ))
    }
}

pub fn ensure_no_lockout_after_regular_key(
    store: &StateStore,
    tx: &DrcRegularKeyTx,
) -> Result<(), StateError> {
    if !drc_master_key_disabled(store, &tx.owner)? {
        return Ok(());
    }
    let after_regular = match tx.action {
        DrcRegularKeyAction::Set => Some(tx.regular_key),
        DrcRegularKeyAction::Clear => None,
    };
    let after_list = load_drc_account_signer_list(store, &tx.owner)?;
    ensure_alternate_recovery(after_regular, after_list.as_ref())
}

pub fn ensure_no_lockout_after_signer_list(
    store: &StateStore,
    tx: &DrcSignerListTx,
) -> Result<(), StateError> {
    if !drc_master_key_disabled(store, &tx.owner)? {
        return Ok(());
    }
    let after_regular = load_drc_account_regular_key(store, &tx.owner)?;
    let after_list = match tx.action {
        DrcSignerListAction::Set => Some(DrcAccountSignerList {
            version: agora_types::DRC_SIGNER_LIST_STATE_VERSION,
            owner: tx.owner,
            quorum: tx.quorum,
            entries: agora_types::canonical_sorted_entries(&tx.entries),
        }),
        DrcSignerListAction::Delete => None,
    };
    ensure_alternate_recovery(after_regular, after_list.as_ref())
}
