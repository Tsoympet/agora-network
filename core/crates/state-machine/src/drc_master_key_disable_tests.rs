//! DRC master-key disable, recovery, and no-lockout invariant tests.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_account_policy_bound, sign_drc_payment_bound, sign_drc_regular_key_bound,
        sign_drc_signer_list_bound, KeyPair,
    };
    use agora_types::{
        canonical_sorted_entries, Amount, DrcAccountPolicyTx, DrcPaymentTx, DrcRegularKeyTx,
        DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId,
        DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::TxAuthContext;
    use crate::drc_account_auth::{
        verify_drc_account_policy_operation, verify_drc_payment_operation,
    };
    use crate::drc_policy::{
        apply_drc_account_policy, drc_account_policy_root, load_drc_account_policy,
    };
    use crate::drc_regular_key::{apply_drc_regular_key, load_drc_account_regular_key};
    use crate::drc_signer_list::apply_drc_signer_list;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn key(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).unwrap()
    }

    fn fund(store: &StateStore, account: &KeyPair, amount: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &account.address(),
            Amount::from_base_units(amount),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn apply_policy(store: &StateStore, tx: &DrcAccountPolicyTx) -> Result<(), crate::StateError> {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(store, tx, &auth(), &mut batch, &mut journal)?;
        store.write_batch(batch)?;
        Ok(())
    }

    fn install_regular(store: &StateStore, master: &KeyPair, regular: &KeyPair) {
        let mut tx = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        sign_drc_regular_key_bound(&mut tx, master, &auth().chain_id, &auth().genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(store, &tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn install_signer_list(store: &StateStore, master: &KeyPair, signer: &KeyPair) {
        let entries = canonical_sorted_entries(&[DrcSignerListEntry {
            signer: signer.address(),
            weight: 1,
        }]);
        let mut tx = DrcSignerListTx::unsigned_set(master.address(), entries, 1, Amount::ZERO, 0);
        sign_drc_signer_list_bound(&mut tx, master, &auth().chain_id, &auth().genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(store, &tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn enable_with_regular_key_recovery_and_reject_no_recovery() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 100);
        install_regular(&store, &master, &regular);

        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &auth().chain_id, &auth().genesis)
            .unwrap();
        apply_policy(&store, &disable).unwrap();
        assert!(
            load_drc_account_policy(&store, &master.address())
                .unwrap()
                .master_key_disabled
        );

        let lone = key(3);
        fund(&store, &lone, 10);
        let mut fail = DrcAccountPolicyTx::set_master_key_disabled(lone.address(), Amount::ZERO, 0);
        sign_drc_account_policy_bound(&mut fail, &lone, &auth().chain_id, &auth().genesis).unwrap();
        assert!(apply_policy(&store, &fail).is_err());
    }

    #[test]
    fn enable_with_signer_list_only() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer = key(2);
        fund(&store, &master, 50);
        install_signer_list(&store, &master, &signer);
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &auth().chain_id, &auth().genesis)
            .unwrap();
        apply_policy(&store, &disable).unwrap();
        assert_eq!(
            load_drc_account_policy(&store, &master.address())
                .unwrap()
                .version,
            DRC_ACCOUNT_POLICY_MASTER_KEY_STATE_VERSION
        );
    }

    #[test]
    fn reject_enable_via_regular_key_or_multisign() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 50);
        install_regular(&store, &master, &regular);

        let mut by_regular =
            DrcAccountPolicyTx::set_master_key_disabled(master.address(), Amount::ZERO, 1);
        sign_drc_account_policy_bound(&mut by_regular, &regular, &auth().chain_id, &auth().genesis)
            .unwrap();
        assert!(verify_drc_account_policy_operation(&store, &by_regular, &auth()).is_err());
    }

    #[test]
    fn disabled_master_cannot_authorize_payment_but_regular_can() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        let payee = key(3);
        fund(&store, &master, 200);
        install_regular(&store, &master, &regular);
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &auth().chain_id, &auth().genesis)
            .unwrap();
        apply_policy(&store, &disable).unwrap();

        let mut bad = DrcPaymentTx::unsigned(
            master.address(),
            payee.address(),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            2,
        );
        sign_drc_payment_bound(&mut bad, &master, &auth().chain_id, &auth().genesis).unwrap();
        assert!(verify_drc_payment_operation(&store, &bad, &auth()).is_err());

        let mut ok = DrcPaymentTx::unsigned(
            master.address(),
            payee.address(),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            2,
        );
        sign_drc_payment_bound(&mut ok, &regular, &auth().chain_id, &auth().genesis).unwrap();
        verify_drc_payment_operation(&store, &ok, &auth()).unwrap();
    }

    #[test]
    fn clear_via_regular_and_reject_master_clear() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 100);
        install_regular(&store, &master, &regular);
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &auth().chain_id, &auth().genesis)
            .unwrap();
        apply_policy(&store, &disable).unwrap();

        let mut master_clear =
            DrcAccountPolicyTx::clear_master_key_disabled(master.address(), Amount::ZERO, 2);
        sign_drc_account_policy_bound(
            &mut master_clear,
            &master,
            &auth().chain_id,
            &auth().genesis,
        )
        .unwrap();
        assert!(verify_drc_account_policy_operation(&store, &master_clear, &auth()).is_err());

        let mut regular_clear =
            DrcAccountPolicyTx::clear_master_key_disabled(master.address(), Amount::ZERO, 2);
        sign_drc_account_policy_bound(
            &mut regular_clear,
            &regular,
            &auth().chain_id,
            &auth().genesis,
        )
        .unwrap();
        apply_policy(&store, &regular_clear).unwrap();
        assert!(
            !load_drc_account_policy(&store, &master.address())
                .unwrap()
                .master_key_disabled
        );

        let mut pay = DrcPaymentTx::unsigned(
            master.address(),
            key(4).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            0,
            Hash::ZERO,
            3,
        );
        sign_drc_payment_bound(&mut pay, &master, &auth().chain_id, &auth().genesis).unwrap();
        verify_drc_payment_operation(&store, &pay, &auth()).unwrap();
    }

    #[test]
    fn no_lockout_blocks_regular_clear_when_only_recovery() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 50);
        install_regular(&store, &master, &regular);
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(1),
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &auth().chain_id, &auth().genesis)
            .unwrap();
        apply_policy(&store, &disable).unwrap();

        let mut clear_key = DrcRegularKeyTx::clear(master.address(), Amount::ZERO, 2);
        sign_drc_regular_key_bound(&mut clear_key, &regular, &auth().chain_id, &auth().genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(
            apply_drc_regular_key(&store, &clear_key, &auth(), &mut batch, &mut journal).is_err()
        );
        assert!(batch.is_empty());
    }

    #[test]
    fn policy_root_changes_with_master_key_disabled_flag() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 20);
        install_regular(&store, &master, &regular);
        let before = drc_account_policy_root(&store).unwrap();
        let mut disable =
            DrcAccountPolicyTx::set_master_key_disabled(master.address(), Amount::ZERO, 1);
        sign_drc_account_policy_bound(&mut disable, &master, &auth().chain_id, &auth().genesis)
            .unwrap();
        apply_policy(&store, &disable).unwrap();
        assert_ne!(drc_account_policy_root(&store).unwrap(), before);
    }
}
