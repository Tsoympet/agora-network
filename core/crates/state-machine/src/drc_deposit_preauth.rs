//! Recipient-controlled, address-based DRC deposit-preauthorization state.

use crate::drc_account_auth::verify_drc_deposit_preauth_operation;
use agora_types::{
    Address, DrcDepositPreauth, DrcDepositPreauthAction, DrcDepositPreauthTx, Hash, NativeAssetId,
};
use borsh::BorshDeserialize;

use crate::accounts::{account_exists, load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_policy::load_drc_account_policy;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const DRC_DEPOSIT_PREAUTH_PREFIX: &[u8] = b"policy/drc/deposit-preauth/";
pub const DRC_DEPOSIT_PREAUTH_ROOT_DOMAIN: &[u8] = b"agora-drc-deposit-preauth-root-v1";

/// Effective point-query result for one recipient/source pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrcDepositAuthorization {
    pub preauthorized: bool,
    pub deposit_auth_required: bool,
    pub deposit_authorized: bool,
}

pub fn drc_deposit_preauth_key(owner: &Address, authorized_source: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        DRC_DEPOSIT_PREAUTH_PREFIX.len() + owner.0.len() + authorized_source.0.len(),
    );
    key.extend_from_slice(DRC_DEPOSIT_PREAUTH_PREFIX);
    key.extend_from_slice(&owner.0);
    key.extend_from_slice(&authorized_source.0);
    key
}

pub fn drc_deposit_preauth_meta_keys(tx: &DrcDepositPreauthTx) -> Vec<Vec<u8>> {
    vec![drc_deposit_preauth_key(&tx.owner, &tx.authorized_source)]
}

/// Direct point lookup; absence is the canonical unauthorized representation.
pub fn load_drc_deposit_preauth(
    store: &StateStore,
    owner: &Address,
    authorized_source: &Address,
) -> Result<bool, StateError> {
    let key = drc_deposit_preauth_key(owner, authorized_source);
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key)? else {
        return Ok(false);
    };
    let record = DrcDepositPreauth::try_from_slice(&bytes)
        .map_err(|error| StateError::Storage(error.to_string()))?;
    record
        .validate()
        .map_err(|error| StateError::Storage(error.to_string()))?;
    if record.owner != *owner || record.authorized_source != *authorized_source {
        return Err(StateError::Storage(
            "DRC deposit-preauthorization record does not match index key".into(),
        ));
    }
    Ok(true)
}

/// Read effective authorization only when both canonical DRC accounts exist.
pub fn load_known_drc_deposit_authorization(
    store: &StateStore,
    owner: &Address,
    authorized_source: &Address,
) -> Result<Option<DrcDepositAuthorization>, StateError> {
    if !account_exists(store, NativeAssetId::DRC, owner)?
        || !account_exists(store, NativeAssetId::DRC, authorized_source)?
    {
        return Ok(None);
    }
    let policy = load_drc_account_policy(store, owner)?;
    let preauthorized = load_drc_deposit_preauth(store, owner, authorized_source)?;
    Ok(Some(DrcDepositAuthorization {
        preauthorized,
        deposit_auth_required: policy.deposit_auth_required,
        deposit_authorized: owner == authorized_source
            || !policy.deposit_auth_required
            || preauthorized,
    }))
}

/// Deterministic commitment over sorted `(recipient, source)` records.
pub fn drc_deposit_preauth_root(store: &StateStore) -> Result<Hash, StateError> {
    let mut entries = Vec::new();
    for (key, bytes) in store.scan_prefix(ColumnFamily::Meta, DRC_DEPOSIT_PREAUTH_PREFIX)? {
        if key.len() != DRC_DEPOSIT_PREAUTH_PREFIX.len() + 40 {
            return Err(StateError::Storage(
                "invalid DRC deposit-preauthorization key length".into(),
            ));
        }
        let mut owner = [0; 20];
        owner.copy_from_slice(
            &key[DRC_DEPOSIT_PREAUTH_PREFIX.len()..DRC_DEPOSIT_PREAUTH_PREFIX.len() + 20],
        );
        let mut source = [0; 20];
        source.copy_from_slice(&key[DRC_DEPOSIT_PREAUTH_PREFIX.len() + 20..]);
        let record = DrcDepositPreauth::try_from_slice(&bytes)
            .map_err(|error| StateError::Storage(error.to_string()))?;
        record
            .validate()
            .map_err(|error| StateError::Storage(error.to_string()))?;
        if record.owner != Address(owner) || record.authorized_source != Address(source) {
            return Err(StateError::Storage(
                "DRC deposit-preauthorization record does not match index key".into(),
            ));
        }
        entries.push(record);
    }
    entries.sort_by_key(|record| (record.owner.0, record.authorized_source.0));
    Ok(Hash::hash_borsh(&(
        DRC_DEPOSIT_PREAUTH_ROOT_DOMAIN,
        entries,
    )))
}

/// Apply one owner-authorized grant or revoke under the shared DRC account nonce.
pub fn apply_drc_deposit_preauth(
    store: &StateStore,
    tx: &DrcDepositPreauthTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<bool, StateError> {
    tx.validate_structure()
        .map_err(|error| StateError::InvalidTx(error.to_string()))?;
    verify_drc_deposit_preauth_operation(store, tx, auth)?;

    if !account_exists(store, NativeAssetId::DRC, &tx.owner)? {
        return Err(StateError::InvalidTx(
            "unknown DRC deposit-preauthorization owner".into(),
        ));
    }
    if !account_exists(store, NativeAssetId::DRC, &tx.authorized_source)? {
        return Err(StateError::InvalidTx(
            "unknown DRC deposit-preauthorization source".into(),
        ));
    }

    let key = drc_deposit_preauth_key(&tx.owner, &tx.authorized_source);
    let exists = load_drc_deposit_preauth(store, &tx.owner, &tx.authorized_source)?;
    match (tx.action, exists) {
        (DrcDepositPreauthAction::Authorize, true) => {
            return Err(StateError::InvalidTx(
                "duplicate DRC deposit preauthorization".into(),
            ));
        }
        (DrcDepositPreauthAction::Unauthorize, false) => {
            return Err(StateError::InvalidTx(
                "missing DRC deposit preauthorization".into(),
            ));
        }
        _ => {}
    }

    let mut owner = load_account(store, NativeAssetId::DRC, &tx.owner)?;
    if owner.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad DRC deposit-preauthorization nonce: got {} expected {}",
            tx.nonce, owner.nonce
        )));
    }
    if owner.balance < tx.fee.as_base_units() {
        return Err(StateError::InvalidTx(
            "insufficient DRC deposit-preauthorization balance".into(),
        ));
    }
    let next_nonce = owner.nonce.checked_add(1).ok_or_else(|| {
        StateError::InvalidTx("DRC deposit-preauthorization nonce overflow".into())
    })?;
    let record = DrcDepositPreauth::new(tx.owner, tx.authorized_source);
    let record_bytes =
        borsh::to_vec(&record).map_err(|error| StateError::Storage(error.to_string()))?;

    journal
        .before
        .push((NativeAssetId::DRC, tx.owner, owner.clone()));
    owner.balance -= tx.fee.as_base_units();
    owner.nonce = next_nonce;
    put_account_into(batch, NativeAssetId::DRC, &tx.owner, &owner)?;
    match tx.action {
        DrcDepositPreauthAction::Authorize => {
            batch.put_cf(ColumnFamily::Meta, &key, &record_bytes);
            Ok(true)
        }
        DrcDepositPreauthAction::Unauthorize => {
            batch.delete_cf(ColumnFamily::Meta, &key);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_drc_deposit_preauth_bound, KeyPair};
    use agora_types::Amount;

    use super::*;
    use crate::accounts::{credit_account_into, load_account, AccountState};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn keypair(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).unwrap()
    }

    fn fund(store: &StateStore, accounts: &[(&KeyPair, u64)]) {
        let mut batch = WriteBatch::new();
        for (account, amount) in accounts {
            credit_account_into(
                &mut batch,
                store,
                NativeAssetId::DRC,
                &account.address(),
                Amount::from_base_units(*amount),
            )
            .unwrap();
        }
        store.write_batch(batch).unwrap();
    }

    fn apply(
        store: &StateStore,
        tx: &DrcDepositPreauthTx,
    ) -> Result<(bool, AccountJournal), StateError> {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let enabled = apply_drc_deposit_preauth(store, tx, &auth(), &mut batch, &mut journal)?;
        store.write_batch(batch)?;
        Ok((enabled, journal))
    }

    #[test]
    fn authorize_unauthorize_fee_nonce_and_root_are_atomic() {
        let store = StateStore::open_in_memory();
        let owner = keypair(1);
        let source = keypair(2);
        fund(&store, &[(&owner, 10), (&source, 1)]);
        let root_before = drc_deposit_preauth_root(&store).unwrap();
        let state_root_before = crate::compose_trident_state_root(&store, &Hash([3; 32])).unwrap();

        let mut grant = DrcDepositPreauthTx::authorize(
            owner.address(),
            source.address(),
            Amount::from_base_units(2),
            0,
        );
        sign_drc_deposit_preauth_bound(&mut grant, &owner, &auth().chain_id, &auth().genesis)
            .unwrap();
        assert!(apply(&store, &grant).unwrap().0);
        assert!(load_drc_deposit_preauth(&store, &owner.address(), &source.address()).unwrap());
        assert_ne!(drc_deposit_preauth_root(&store).unwrap(), root_before);
        assert_ne!(
            crate::compose_trident_state_root(&store, &Hash([3; 32])).unwrap(),
            state_root_before
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            AccountState {
                balance: 8,
                nonce: 1
            }
        );

        let before_duplicate = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert!(apply(&store, &grant).is_err());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            before_duplicate
        );

        let mut revoke = DrcDepositPreauthTx::unauthorize(
            owner.address(),
            source.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_deposit_preauth_bound(&mut revoke, &owner, &auth().chain_id, &auth().genesis)
            .unwrap();
        assert!(!apply(&store, &revoke).unwrap().0);
        assert!(!load_drc_deposit_preauth(&store, &owner.address(), &source.address()).unwrap());
        assert_eq!(drc_deposit_preauth_root(&store).unwrap(), root_before);
    }

    #[test]
    fn wrong_owner_tamper_unknown_self_zero_and_missing_revoke_stage_no_state() {
        let store = StateStore::open_in_memory();
        let owner = keypair(1);
        let source = keypair(2);
        let unknown = keypair(3);
        fund(&store, &[(&owner, 10), (&source, 1)]);

        let mut cases = Vec::new();
        let mut missing = DrcDepositPreauthTx::unauthorize(
            owner.address(),
            source.address(),
            Amount::from_base_units(1),
            0,
        );
        sign_drc_deposit_preauth_bound(&mut missing, &owner, &auth().chain_id, &auth().genesis)
            .unwrap();
        cases.push(missing);

        let mut unknown_source =
            DrcDepositPreauthTx::authorize(owner.address(), unknown.address(), Amount::ZERO, 0);
        sign_drc_deposit_preauth_bound(
            &mut unknown_source,
            &owner,
            &auth().chain_id,
            &auth().genesis,
        )
        .unwrap();
        cases.push(unknown_source);

        let mut tampered =
            DrcDepositPreauthTx::authorize(owner.address(), source.address(), Amount::ZERO, 0);
        sign_drc_deposit_preauth_bound(&mut tampered, &owner, &auth().chain_id, &auth().genesis)
            .unwrap();
        tampered.owner = source.address();
        cases.push(tampered);

        for tx in cases {
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            assert!(
                apply_drc_deposit_preauth(&store, &tx, &auth(), &mut batch, &mut journal).is_err()
            );
            assert!(batch.is_empty());
            assert!(journal.before.is_empty());
        }

        for mut invalid in [
            DrcDepositPreauthTx::authorize(owner.address(), owner.address(), Amount::ZERO, 0),
            DrcDepositPreauthTx::authorize(owner.address(), Address::ZERO, Amount::ZERO, 0),
        ] {
            invalid.public_key = vec![2; 33];
            invalid.signature = vec![3; 64];
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            assert!(
                apply_drc_deposit_preauth(&store, &invalid, &auth(), &mut batch, &mut journal)
                    .is_err()
            );
            assert!(batch.is_empty());
        }
    }

    #[test]
    fn legacy_and_future_persisted_record_versions_fail_closed() {
        for version in [0, agora_types::DRC_DEPOSIT_PREAUTH_STATE_VERSION + 1] {
            let store = StateStore::open_in_memory();
            let owner = Address([1; 20]);
            let source = Address([2; 20]);
            let record = DrcDepositPreauth {
                version,
                owner,
                authorized_source: source,
            };
            store
                .put_cf(
                    ColumnFamily::Meta,
                    &drc_deposit_preauth_key(&owner, &source),
                    &borsh::to_vec(&record).unwrap(),
                )
                .unwrap();
            assert!(load_drc_deposit_preauth(&store, &owner, &source).is_err());
            assert!(drc_deposit_preauth_root(&store).is_err());
        }
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn preauthorization_persists_across_rocksdb_reopen() {
        let directory = std::env::temp_dir().join(format!(
            "agora-drc-deposit-preauth-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let owner = keypair(1);
        let source = keypair(2);
        {
            let store = StateStore::open(&directory).unwrap();
            fund(&store, &[(&owner, 10), (&source, 1)]);
            let mut grant = DrcDepositPreauthTx::authorize(
                owner.address(),
                source.address(),
                Amount::from_base_units(1),
                0,
            );
            sign_drc_deposit_preauth_bound(&mut grant, &owner, &auth().chain_id, &auth().genesis)
                .unwrap();
            apply(&store, &grant).unwrap();
        }
        let reopened = StateStore::open(&directory).unwrap();
        assert!(load_drc_deposit_preauth(&reopened, &owner.address(), &source.address()).unwrap());
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
