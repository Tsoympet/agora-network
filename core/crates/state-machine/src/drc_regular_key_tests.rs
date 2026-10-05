//! Integration tests for DRC regular-key rotation and authorization.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_account_policy_bound, sign_drc_payment_bound, sign_drc_regular_key_bound, KeyPair,
    };
    use agora_types::{
        Amount, DrcPaymentTx, DrcRegularKeyTx, Hash, NativeAssetId,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::TxAuthContext;
    use crate::drc_account_auth::authorize_drc_account_operator;
    use crate::drc_policy::load_drc_account_policy;
    use crate::drc_regular_key::{apply_drc_regular_key, load_drc_account_regular_key};
    use crate::payments::apply_drc_payment;
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

    fn apply_key(store: &StateStore, tx: &DrcRegularKeyTx) {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(store, tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn master_set_regular_key_and_regular_key_pays() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        let recipient = key(3);
        fund(&store, &master, 100);
        let auth = auth();

        let mut set = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::from_base_units(1),
            0,
        );
        sign_drc_regular_key_bound(&mut set, &master, &auth.chain_id, &auth.genesis).unwrap();
        apply_key(&store, &set);
        assert_eq!(
            load_drc_account_regular_key(&store, &master.address()).unwrap(),
            Some(regular.address())
        );

        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            recipient.address(),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            1,
        );
        sign_drc_payment_bound(&mut payment, &regular, &auth.chain_id, &auth.genesis).unwrap();
        authorize_drc_account_operator(&store, &master.address(), &regular.address()).unwrap();

        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &payment, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            5
        );
    }

    #[test]
    fn regular_key_rotates_itself_and_master_recovers() {
        let store = StateStore::open_in_memory();
        let master = key(10);
        let regular_a = key(11);
        let regular_b = key(12);
        fund(&store, &master, 50);
        let auth = auth();

        install_key(&store, &master, &regular_a, &auth, 0);
        let mut rotate = DrcRegularKeyTx::set(
            master.address(),
            regular_b.address(),
            regular_b.public_key_bytes().to_vec(),
            Amount::ZERO,
            1,
        );
        sign_drc_regular_key_bound(&mut rotate, &regular_a, &auth.chain_id, &auth.genesis).unwrap();
        apply_key(&store, &rotate);
        assert_eq!(
            load_drc_account_regular_key(&store, &master.address()).unwrap(),
            Some(regular_b.address())
        );

        let mut clear = DrcRegularKeyTx::clear(master.address(), Amount::ZERO, 2);
        sign_drc_regular_key_bound(&mut clear, &master, &auth.chain_id, &auth.genesis).unwrap();
        apply_key(&store, &clear);
        assert_eq!(
            load_drc_account_regular_key(&store, &master.address()).unwrap(),
            None
        );
    }

    #[test]
    fn same_block_overlay_set_then_policy_uses_new_signer() {
        let store = StateStore::open_in_memory();
        let master = key(20);
        let regular = key(21);
        fund(&store, &master, 20);
        let auth = auth();
        let lane = store.cow_overlay();

        let mut set = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        sign_drc_regular_key_bound(&mut set, &master, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(&lane, &set, &auth, &mut batch, &mut journal).unwrap();
        lane.write_batch(batch).unwrap();

        let mut policy = agora_types::DrcAccountPolicyTx::set_require_destination_tag(
            master.address(),
            Amount::ZERO,
            1,
        );
        sign_drc_account_policy_bound(&mut policy, &regular, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(
            &lane,
            &policy,
            &auth,
            &mut batch,
            &mut journal,
        )
        .unwrap();
        lane.write_batch(batch).unwrap();

        assert!(
            load_drc_account_policy(&lane, &master.address())
                .unwrap()
                .require_destination_tag
        );
    }

    fn install_key(
        store: &StateStore,
        master: &KeyPair,
        regular: &KeyPair,
        auth: &TxAuthContext,
        nonce: u64,
    ) {
        let mut set = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            nonce,
        );
        sign_drc_regular_key_bound(&mut set, master, &auth.chain_id, &auth.genesis).unwrap();
        apply_key(store, &set);
    }
}
