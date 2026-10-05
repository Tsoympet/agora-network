//! Recipient-controlled DRC account-policy state and transition.

use agora_crypto::verify_drc_account_policy_bound;
use agora_types::{
    Address, DrcAccountPolicy, DrcAccountPolicyTx, Hash, NativeAssetId,
    DRC_ACCOUNT_POLICY_STATE_VERSION,
};
use borsh::BorshDeserialize;

use crate::accounts::{account_key, load_account, put_account_into, AccountJournal, AccountState};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const DRC_ACCOUNT_POLICY_PREFIX: &[u8] = b"policy/drc/account/";
pub const DRC_ACCOUNT_POLICY_ROOT_DOMAIN: &[u8] = b"agora-drc-account-policy-root-v1";

pub fn drc_account_policy_key(account: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(DRC_ACCOUNT_POLICY_PREFIX.len() + account.0.len());
    key.extend_from_slice(DRC_ACCOUNT_POLICY_PREFIX);
    key.extend_from_slice(&account.0);
    key
}

pub fn drc_account_policy_meta_keys(tx: &DrcAccountPolicyTx) -> Vec<Vec<u8>> {
    vec![drc_account_policy_key(&tx.account)]
}

/// Missing policy state is the consensus default: destination tags are optional.
pub fn load_drc_account_policy(
    store: &StateStore,
    account: &Address,
) -> Result<DrcAccountPolicy, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &drc_account_policy_key(account))? else {
        return Ok(DrcAccountPolicy::default());
    };
    let policy = DrcAccountPolicy::try_from_slice(&bytes)
        .map_err(|error| StateError::Storage(error.to_string()))?;
    if policy.version != DRC_ACCOUNT_POLICY_STATE_VERSION {
        return Err(StateError::Storage(format!(
            "unsupported DRC account-policy state version {}",
            policy.version
        )));
    }
    Ok(policy)
}

/// Read policy only for an account that exists in canonical DRC state.
pub fn load_known_drc_account_policy(
    store: &StateStore,
    account: &Address,
) -> Result<Option<(DrcAccountPolicy, AccountState)>, StateError> {
    let policy_key = drc_account_policy_key(account);
    let account_key = account_key(NativeAssetId::DRC, account);
    if store.get_cf(ColumnFamily::Meta, &account_key)?.is_none()
        && store.get_cf(ColumnFamily::Meta, &policy_key)?.is_none()
    {
        return Ok(None);
    }
    Ok(Some((
        load_drc_account_policy(store, account)?,
        load_account(store, NativeAssetId::DRC, account)?,
    )))
}

/// Deterministic commitment over sorted recipient-policy entries.
pub fn drc_account_policy_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, DRC_ACCOUNT_POLICY_PREFIX)? {
        if key.len() != DRC_ACCOUNT_POLICY_PREFIX.len() + 20 {
            return Err(StateError::Storage(
                "invalid DRC account-policy key length".into(),
            ));
        }
        let mut account = [0; 20];
        account.copy_from_slice(&key[DRC_ACCOUNT_POLICY_PREFIX.len()..]);
        let policy = DrcAccountPolicy::try_from_slice(&bytes)
            .map_err(|error| StateError::Storage(error.to_string()))?;
        if policy.version != DRC_ACCOUNT_POLICY_STATE_VERSION {
            return Err(StateError::Storage(format!(
                "unsupported DRC account-policy state version {}",
                policy.version
            )));
        }
        entries.push((Address(account), policy));
    }
    entries.sort_by_key(|(account, _)| account.0);
    Ok(Hash::hash_borsh(&(DRC_ACCOUNT_POLICY_ROOT_DOMAIN, entries)))
}

/// Apply one owner-authorized policy operation under the shared DRC nonce.
pub fn apply_drc_account_policy(
    store: &StateStore,
    tx: &DrcAccountPolicyTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcAccountPolicy, StateError> {
    tx.validate_version()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    verify_drc_account_policy_bound(tx, &auth.chain_id, &auth.genesis)
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;

    let mut account = load_account(store, NativeAssetId::DRC, &tx.account)?;
    if account.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad DRC account-policy nonce: got {} expected {}",
            tx.nonce, account.nonce
        )));
    }
    if account.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC account-policy balance".into(),
        ));
    }
    let next_nonce = account
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC account-policy nonce overflow".into()))?;
    let policy = DrcAccountPolicy {
        version: DRC_ACCOUNT_POLICY_STATE_VERSION,
        require_destination_tag: tx.action.require_destination_tag(),
    };
    let policy_bytes =
        borsh::to_vec(&policy).map_err(|error| StateError::Storage(error.to_string()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.account, account.clone()));
    account.balance -= tx.fee.as_base_units();
    account.nonce = next_nonce;
    put_account_into(batch, NativeAssetId::DRC, &tx.account, &account)?;
    if policy.require_destination_tag {
        batch.put_cf(
            ColumnFamily::Meta,
            &drc_account_policy_key(&tx.account),
            &policy_bytes,
        );
    } else {
        // Absence is the canonical false representation, avoiding two roots for
        // semantically identical default-off state.
        batch.delete_cf(ColumnFamily::Meta, &drc_account_policy_key(&tx.account));
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_drc_account_policy_bound, KeyPair};
    use agora_types::{Amount, DrcAccountPolicyTx};

    use super::*;
    use crate::accounts::{credit_account_into, load_account};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn owner() -> KeyPair {
        KeyPair::from_secret_bytes(&[1; 32]).unwrap()
    }

    fn fund(store: &StateStore, owner: &KeyPair) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &owner.address(),
            Amount::from_base_units(10),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn apply(store: &StateStore, tx: &DrcAccountPolicyTx) -> Result<DrcAccountPolicy, StateError> {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let policy = apply_drc_account_policy(store, tx, &auth(), &mut batch, &mut journal)?;
        store.write_batch(batch)?;
        Ok(policy)
    }

    #[test]
    fn default_off_set_clear_and_replay_rejection() {
        let store = StateStore::open_in_memory();
        let owner = owner();
        fund(&store, &owner);
        assert!(
            !load_drc_account_policy(&store, &owner.address())
                .unwrap()
                .require_destination_tag
        );
        let root_before = drc_account_policy_root(&store).unwrap();
        let state_root_before = crate::compose_trident_state_root(&store, &Hash([3; 32])).unwrap();

        let mut set = DrcAccountPolicyTx::set_require_destination_tag(
            owner.address(),
            Amount::from_base_units(2),
            0,
        );
        sign_drc_account_policy_bound(&mut set, &owner, &auth().chain_id, &auth().genesis).unwrap();
        assert!(apply(&store, &set).unwrap().require_destination_tag);
        assert_ne!(drc_account_policy_root(&store).unwrap(), root_before);
        assert_ne!(
            crate::compose_trident_state_root(&store, &Hash([3; 32])).unwrap(),
            state_root_before
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            1
        );
        assert!(apply(&store, &set).is_err());

        let mut clear = DrcAccountPolicyTx::clear_require_destination_tag(
            owner.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_account_policy_bound(&mut clear, &owner, &auth().chain_id, &auth().genesis)
            .unwrap();
        assert!(!apply(&store, &clear).unwrap().require_destination_tag);
        assert_eq!(drc_account_policy_root(&store).unwrap(), root_before);
        assert!(
            !load_drc_account_policy(&store, &owner.address())
                .unwrap()
                .require_destination_tag
        );
    }

    #[test]
    fn wrong_owner_and_tampering_stage_no_state() {
        let store = StateStore::open_in_memory();
        let owner = owner();
        let other = KeyPair::from_secret_bytes(&[2; 32]).unwrap();
        fund(&store, &owner);
        let mut tx = DrcAccountPolicyTx::set_require_destination_tag(
            owner.address(),
            Amount::from_base_units(1),
            0,
        );
        sign_drc_account_policy_bound(&mut tx, &owner, &auth().chain_id, &auth().genesis).unwrap();
        tx.account = other.address();

        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_account_policy(&store, &tx, &auth(), &mut batch, &mut journal).is_err());
        assert!(batch.is_empty());
        assert!(journal.before.is_empty());
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn policy_persists_across_rocksdb_reopen() {
        let directory = std::env::temp_dir().join(format!(
            "agora-drc-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let owner = owner();
        {
            let store = StateStore::open(&directory).unwrap();
            fund(&store, &owner);
            let mut tx = DrcAccountPolicyTx::set_require_destination_tag(
                owner.address(),
                Amount::from_base_units(1),
                0,
            );
            sign_drc_account_policy_bound(&mut tx, &owner, &auth().chain_id, &auth().genesis)
                .unwrap();
            apply(&store, &tx).unwrap();
        }
        let reopened = StateStore::open(&directory).unwrap();
        assert!(
            load_drc_account_policy(&reopened, &owner.address())
                .unwrap()
                .require_destination_tag
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
