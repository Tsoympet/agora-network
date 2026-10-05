//! Security hardening: per-operation disabled-master rejection, recovery, mined blocks, invariants.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_account_transfer_bound, sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound,
        sign_drc_multisign_participant_bound, sign_drc_payment_bound, sign_drc_regular_key_bound,
        sign_drc_signer_list_bound, sign_stake_tx_bound, KeyPair,
    };
    use agora_types::{
        materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane,
        AccountTransfer, Amount, Block, BlockHeader, DrcAccountPolicyTx,
        DrcDepositPreauthTx, DrcMultisignAuth, DrcMultisignEntry, DrcMultisignOperationKind,
        DrcPaymentTx, DrcRegularKeyTx, DrcSignerListEntry, DrcSignerListTx,
        Hash, NativeAssetId, SignedStakeTx, Transaction, TxOut,
        DRC_MULTISIGN_AUTH_VERSION,
    };
    use borsh::BorshDeserialize;

    use crate::accounts::{apply_account_transfer, credit_account_into, load_account};
    use crate::apply::{
        apply_block_batched_with_auth, apply_block_batched_with_auth_at_blue_score,
        revert_journal_batched, TxAuthContext,
    };
    use crate::drc_account_auth::{
        verify_drc_account_policy_operation, verify_drc_account_transfer_operation,
        verify_drc_deposit_preauth_operation, verify_drc_payment_operation,
        verify_drc_regular_key_operation, verify_drc_signer_list_operation,
        verify_drc_stake_operation,
    };
    use crate::drc_deposit_preauth::{apply_drc_deposit_preauth, load_drc_deposit_preauth};
    use crate::drc_master_key_recovery::{assert_drc_recovery_invariant, drc_master_key_disabled};
    use crate::drc_policy::{
        apply_drc_account_policy, drc_account_policy_root, load_drc_account_policy,
    };
    use crate::drc_regular_key::{apply_drc_regular_key, load_drc_account_regular_key};
    use crate::drc_signer_list::{apply_drc_signer_list, load_drc_account_signer_list};
    use crate::payments::apply_drc_payment;
    use crate::staking::{apply_signed_stake_tx, StakingParams};
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateError, StateStore};

    const TIP: Hash = Hash([4; 32]);

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn key(b: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[b; 32]).unwrap()
    }

    fn fund(store: &StateStore, kp: &KeyPair, amount: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(amount),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    struct Snap {
        nonce: u64,
        balance: u64,
        policy_root: Hash,
        state_root: Hash,
        disabled: bool,
        regular: Option<agora_types::Address>,
        list: bool,
    }

    fn snap(store: &StateStore, owner: &agora_types::Address) -> Snap {
        let acct = load_account(store, NativeAssetId::DRC, owner).unwrap();
        Snap {
            nonce: acct.nonce,
            balance: acct.balance,
            policy_root: drc_account_policy_root(store).unwrap(),
            state_root: compose_trident_state_root(store, &TIP).unwrap(),
            disabled: load_drc_account_policy(store, owner)
                .unwrap()
                .master_key_disabled,
            regular: load_drc_account_regular_key(store, owner).unwrap(),
            list: load_drc_account_signer_list(store, owner)
                .unwrap()
                .is_some(),
        }
    }

    fn assert_snap_unchanged(store: &StateStore, owner: &agora_types::Address, before: &Snap) {
        let after = snap(store, owner);
        assert_eq!(after.nonce, before.nonce);
        assert_eq!(after.balance, before.balance);
        assert_eq!(after.policy_root, before.policy_root);
        assert_eq!(after.state_root, before.state_root);
        assert_eq!(after.disabled, before.disabled);
        assert_eq!(after.regular, before.regular);
        assert_eq!(after.list, before.list);
    }

    fn assert_apply_atomic_fail<F, T>(
        store: &StateStore,
        owner: &agora_types::Address,
        before: &Snap,
        f: F,
    ) where
        F: FnOnce(&mut WriteBatch, &mut AccountJournal) -> Result<T, StateError>,
    {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(f(&mut batch, &mut journal).is_err());
        assert!(batch.is_empty());
        assert!(journal.before.is_empty());
        assert_snap_unchanged(store, owner, before);
    }

    fn install_regular(store: &StateStore, master: &KeyPair, regular: &KeyPair, nonce: u64) {
        let mut tx = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            nonce,
        );
        sign_drc_regular_key_bound(&mut tx, master, &auth().chain_id, &auth().genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(store, &tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn install_list(store: &StateStore, master: &KeyPair, signer: &KeyPair, nonce: u64) {
        let entries = agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
            signer: signer.address(),
            weight: 1,
        }]);
        let mut tx =
            DrcSignerListTx::unsigned_set(master.address(), entries, 1, Amount::ZERO, nonce);
        sign_drc_signer_list_bound(&mut tx, master, &auth().chain_id, &auth().genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(store, &tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn disable_master(store: &StateStore, master: &KeyPair, nonce: u64) {
        let mut tx = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(1),
            nonce,
        );
        sign_drc_account_policy_bound(&mut tx, master, &auth().chain_id, &auth().genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(store, &tx, &auth(), &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn setup_disabled_with_both(
        store: &StateStore,
        master: &KeyPair,
        regular: &KeyPair,
        signer: &KeyPair,
    ) {
        fund(store, master, 1_000);
        install_regular(store, master, regular, 0);
        install_list(store, master, signer, 1);
        disable_master(store, master, 2);
        assert!(drc_master_key_disabled(store, &master.address()).unwrap());
        assert_drc_recovery_invariant(store, &master.address()).unwrap();
    }

    fn multisign_auth(
        owner: agora_types::Address,
        signing_bytes: &[u8],
        signers: &[&KeyPair],
        ctx: &TxAuthContext,
    ) -> DrcMultisignAuth {
        let mut entries = Vec::new();
        for kp in signers {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                owner,
                signing_bytes,
                kp,
                &ctx.chain_id,
                &ctx.genesis,
            )
            .unwrap();
            entries.push(DrcMultisignEntry {
                signer,
                public_key,
                signature,
            });
        }
        entries.sort_by_key(|e| e.signer.0);
        DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: owner,
            signatures: entries,
        }
    }

    fn coinbase_block(parent: Hash, nonce: u64) -> Block {
        Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![parent],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![Transaction::unsigned(
                1,
                vec![],
                vec![TxOut {
                    value: Amount::from_base_units(50),
                    address: key(99).address(),
                }],
                nonce,
            )],
        )
    }

    #[test]
    fn disabled_master_rejected_for_every_operation_class_atomically() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        let signer = key(3);
        let peer = key(4);
        let source = key(5);
        setup_disabled_with_both(&store, &master, &regular, &signer);
        let owner = master.address();
        let ctx = auth();
        let before = snap(&store, &owner);

        let mut transfer = AccountTransfer::unsigned_with_fee(
            NativeAssetId::DRC,
            owner,
            peer.address(),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            3,
        );
        sign_account_transfer_bound(&mut transfer, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_account_transfer_operation(&store, &transfer, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_account_transfer(&store, &transfer, &ctx, batch, journal)
        });

        let params = StakingParams::drc_default();
        let bond = params.min_self_bond;
        let mut stake = SignedStakeTx::unsigned_bond(
            NativeAssetId::DRC,
            owner,
            bond,
            master.public_key_bytes().to_vec(),
            owner,
            0,
            3,
        );
        sign_stake_tx_bound(&mut stake, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_stake_operation(&store, &stake, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, _| {
            apply_signed_stake_tx(&store, batch, &stake, &ctx, &params)
        });

        let mut rk = DrcRegularKeyTx::set(
            owner,
            peer.address(),
            peer.public_key_bytes().to_vec(),
            Amount::ZERO,
            3,
        );
        sign_drc_regular_key_bound(&mut rk, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_regular_key_operation(&store, &rk, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_regular_key(&store, &rk, &ctx, batch, journal)
        });

        let entries = agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
            signer: peer.address(),
            weight: 1,
        }]);
        let mut sl = DrcSignerListTx::unsigned_set(owner, entries, 1, Amount::ZERO, 3);
        sign_drc_signer_list_bound(&mut sl, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_signer_list_operation(&store, &sl, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_signer_list(&store, &sl, &ctx, batch, journal)
        });

        let mut sl_del = DrcSignerListTx::unsigned_delete(owner, Amount::ZERO, 3);
        sign_drc_signer_list_bound(&mut sl_del, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_signer_list_operation(&store, &sl_del, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_signer_list(&store, &sl_del, &ctx, batch, journal)
        });

        let mut pol = DrcAccountPolicyTx::set_require_destination_tag(owner, Amount::ZERO, 3);
        sign_drc_account_policy_bound(&mut pol, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_account_policy_operation(&store, &pol, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_account_policy(&store, &pol, &ctx, batch, journal)
        });

        let mut pre = DrcDepositPreauthTx::authorize(owner, source.address(), Amount::ZERO, 3);
        sign_drc_deposit_preauth_bound(&mut pre, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_deposit_preauth_operation(&store, &pre, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_deposit_preauth(&store, &pre, &ctx, batch, journal)
        });

        let mut pre_revoke =
            DrcDepositPreauthTx::unauthorize(owner, source.address(), Amount::ZERO, 3);
        sign_drc_deposit_preauth_bound(&mut pre_revoke, &master, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        assert!(verify_drc_deposit_preauth_operation(&store, &pre_revoke, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_deposit_preauth(&store, &pre_revoke, &ctx, batch, journal)
        });

        let mut pay = DrcPaymentTx::unsigned(
            owner,
            peer.address(),
            Amount::from_base_units(2),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            3,
        );
        sign_drc_payment_bound(&mut pay, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_payment_operation(&store, &pay, &ctx).is_err());
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_payment(&store, &pay, &ctx, batch, journal)
        });
    }

    #[test]
    fn recovery_regular_and_multisign_succeed_while_master_disabled() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        let signer = key(3);
        let peer = key(4);
        let source = key(5);
        setup_disabled_with_both(&store, &master, &regular, &signer);
        let owner = master.address();
        let ctx = auth();
        let params = StakingParams::drc_default();

        let mut transfer = AccountTransfer::unsigned_with_fee(
            NativeAssetId::DRC,
            owner,
            peer.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            3,
        );
        sign_account_transfer_bound(&mut transfer, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_account_transfer_operation(&store, &transfer, &ctx).unwrap();

        let mut stake = SignedStakeTx::unsigned_bond(
            NativeAssetId::DRC,
            owner,
            params.min_self_bond,
            regular.public_key_bytes().to_vec(),
            owner,
            0,
            3,
        );
        sign_stake_tx_bound(&mut stake, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_stake_operation(&store, &stake, &ctx).unwrap();

        let mut pol = DrcAccountPolicyTx::set_require_destination_tag(owner, Amount::ZERO, 3);
        sign_drc_account_policy_bound(&mut pol, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_account_policy_operation(&store, &pol, &ctx).unwrap();

        let mut pre = DrcDepositPreauthTx::authorize(owner, source.address(), Amount::ZERO, 3);
        sign_drc_deposit_preauth_bound(&mut pre, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_deposit_preauth_operation(&store, &pre, &ctx).unwrap();

        let mut pay = DrcPaymentTx::unsigned(
            owner,
            peer.address(),
            Amount::from_base_units(3),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            3,
        );
        sign_drc_payment_bound(&mut pay, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_payment_operation(&store, &pay, &ctx).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &pay, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner)
                .unwrap()
                .nonce,
            4
        );

        let mut pay_ms = DrcPaymentTx::unsigned(
            owner,
            peer.address(),
            Amount::from_base_units(2),
            Amount::ZERO,
            0,
            Hash::ZERO,
            4,
        );
        pay_ms.public_key.clear();
        pay_ms.signature.clear();
        pay_ms.multisign = Some(multisign_auth(
            owner,
            &pay_ms.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        verify_drc_payment_operation(&store, &pay_ms, &ctx).unwrap();

        let entries = agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
            signer: peer.address(),
            weight: 1,
        }]);
        let mut sl = DrcSignerListTx::unsigned_set(owner, entries, 1, Amount::ZERO, 5);
        sl.public_key.clear();
        sl.signature.clear();
        sl.multisign = Some(multisign_auth(
            owner,
            &sl.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        verify_drc_signer_list_operation(&store, &sl, &ctx).unwrap();

        let mut rk = DrcRegularKeyTx::set(
            owner,
            peer.address(),
            peer.public_key_bytes().to_vec(),
            Amount::ZERO,
            5,
        );
        sign_drc_regular_key_bound(&mut rk, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_regular_key_operation(&store, &rk, &ctx).unwrap();
    }

    #[test]
    fn signer_list_only_multisign_clear_master_disabled_mined_block_roundtrip() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer = key(2);
        fund(&store, &master, 300);
        install_list(&store, &master, &signer, 0);
        disable_master(&store, &master, 1);
        let ctx = auth();
        let owner = master.address();

        let mut clear =
            DrcAccountPolicyTx::clear_master_key_disabled(owner, Amount::from_base_units(1), 2);
        clear.public_key.clear();
        clear.signature.clear();
        clear.multisign = Some(multisign_auth(
            owner,
            &clear.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));

        let mut block = coinbase_block(Hash::ZERO, 1);
        block.drc_account_policies = vec![clear];
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();

        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, decoded.compute_body_root());
        validate_drc_multisign_attachment_lane(&decoded, &ctx.chain_id, &ctx.genesis).unwrap();

        let result = apply_block_batched_with_auth(&store, &decoded, 50, Some(&ctx)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(
            !load_drc_account_policy(&store, &owner)
                .unwrap()
                .master_key_disabled
        );

        let mut pay = DrcPaymentTx::unsigned(
            owner,
            key(7).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            0,
            Hash::ZERO,
            3,
        );
        sign_drc_payment_bound(&mut pay, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        verify_drc_payment_operation(&store, &pay, &ctx).unwrap();
    }

    #[test]
    fn clear_master_policy_attachment_negatives() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer = key(2);
        fund(&store, &master, 100);
        install_list(&store, &master, &signer, 0);
        disable_master(&store, &master, 1);
        let ctx = auth();

        let mut clear =
            DrcAccountPolicyTx::clear_master_key_disabled(master.address(), Amount::ZERO, 2);
        clear.public_key.clear();
        clear.signature.clear();
        clear.multisign = Some(multisign_auth(
            master.address(),
            &clear.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        let mut block = coinbase_block(Hash::ZERO, 2);
        block.drc_account_policies = vec![clear.clone()];
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcPayment;
        assert!(
            validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).is_err()
        );

        let mut missing = block.clone();
        missing.drc_multisign_attachments.clear();
        missing.drc_account_policies[0].multisign = Some(clear.multisign.clone().unwrap());
        assert!(apply_block_batched_with_auth(&store, &missing, 50, Some(&ctx)).is_err());
    }

    #[test]
    fn same_block_regular_then_disable_preserves_recovery() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 200);
        let ctx = auth();

        let mut rk = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        sign_drc_regular_key_bound(&mut rk, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut disable =
            DrcAccountPolicyTx::set_master_key_disabled(master.address(), Amount::ZERO, 1);
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut block = coinbase_block(Hash::ZERO, 3);
        block.drc_regular_keys = vec![rk];
        block.drc_account_policies = vec![disable];
        block.header.tx_root = block.compute_body_root();

        let result = apply_block_batched_with_auth(&store, &block, 50, Some(&ctx)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(drc_master_key_disabled(&store, &master.address()).unwrap());
        assert_drc_recovery_invariant(&store, &master.address()).unwrap();
    }

    #[test]
    fn same_block_master_transfer_fails_while_disabled() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        setup_disabled_with_both(&store, &master, &regular, &key(3));
        let ctx = auth();

        let mut transfer = AccountTransfer::unsigned_with_fee(
            NativeAssetId::DRC,
            master.address(),
            key(8).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            3,
        );
        sign_account_transfer_bound(&mut transfer, &master, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut block = coinbase_block(Hash::ZERO, 4);
        block.account_transfers = vec![transfer];
        block.header.tx_root = block.compute_body_root();
        assert!(apply_block_batched_with_auth(&store, &block, 50, Some(&ctx)).is_err());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &master.address())
                .unwrap()
                .nonce,
            3
        );
    }

    #[test]
    fn revert_block_journal_restores_disable_policy_and_state_root() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        fund(&store, &master, 100);
        install_regular(&store, &master, &regular, 0);
        let ctx = auth();
        let root_before = compose_trident_state_root(&store, &TIP).unwrap();

        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::from_base_units(2),
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase_block(Hash::ZERO, 6);
        block.drc_account_policies = vec![disable];
        block.header.tx_root = block.compute_body_root();

        let result = apply_block_batched_with_auth(&store, &block, 50, Some(&ctx)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(drc_master_key_disabled(&store, &master.address()).unwrap());
        assert_ne!(
            compose_trident_state_root(&store, &TIP).unwrap(),
            root_before
        );

        store
            .write_batch(revert_journal_batched(&result.journal).unwrap())
            .unwrap();
        assert!(!drc_master_key_disabled(&store, &master.address()).unwrap());
        assert_eq!(
            compose_trident_state_root(&store, &TIP).unwrap(),
            root_before
        );
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_reopen_preserves_disable_and_auth_decisions() {
        let dir = std::env::temp_dir().join(format!(
            "agora-drc-master-disable-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let master = key(1);
        let regular = key(2);
        {
            let store = StateStore::open(&dir).unwrap();
            fund(&store, &master, 80);
            install_regular(&store, &master, &regular, 0);
            disable_master(&store, &master, 1);
        }
        let store = StateStore::open(&dir).unwrap();
        assert!(drc_master_key_disabled(&store, &master.address()).unwrap());
        let mut pay = DrcPaymentTx::unsigned(
            master.address(),
            key(9).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            0,
            Hash::ZERO,
            2,
        );
        sign_drc_payment_bound(&mut pay, &master, &auth().chain_id, &auth().genesis).unwrap();
        assert!(verify_drc_payment_operation(&store, &pay, &auth()).is_err());
        sign_drc_payment_bound(&mut pay, &regular, &auth().chain_id, &auth().genesis).unwrap();
        verify_drc_payment_operation(&store, &pay, &auth()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn composition_tags_deposit_auth_expiry_multisign_block() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let signer = key(2);
        let source = key(3);
        let payer = key(4);
        fund(&store, &owner, 400);
        fund(&store, &source, 200);
        install_list(&store, &owner, &signer, 0);
        disable_master(&store, &owner, 1);

        let ctx = auth();
        let mut tag =
            DrcAccountPolicyTx::set_require_destination_tag(owner.address(), Amount::ZERO, 2);
        tag.public_key.clear();
        tag.signature.clear();
        tag.multisign = Some(multisign_auth(
            owner.address(),
            &tag.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        let mut enable =
            DrcAccountPolicyTx::set_deposit_auth_required(owner.address(), Amount::ZERO, 3);
        enable.public_key.clear();
        enable.signature.clear();
        enable.multisign = Some(multisign_auth(
            owner.address(),
            &enable.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));

        let mut grant =
            DrcDepositPreauthTx::authorize(owner.address(), source.address(), Amount::ZERO, 4);
        grant.public_key.clear();
        grant.signature.clear();
        grant.multisign = Some(multisign_auth(
            owner.address(),
            &grant.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));

        let mut payment = DrcPaymentTx::unsigned_v4(
            source.address(),
            owner.address(),
            Amount::from_base_units(5),
            Amount::from_base_units(1),
            Some(42),
            None,
            Hash::ZERO,
            0,
            Some(100),
        );
        sign_drc_payment_bound(&mut payment, &source, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut block = coinbase_block(Hash::ZERO, 5);
        block.drc_signer_lists = vec![];
        block.drc_account_policies = vec![tag, enable];
        block.drc_deposit_preauths = vec![grant];
        block.drc_payments = vec![payment];
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();

        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(load_drc_deposit_preauth(&store, &owner.address(), &source.address()).unwrap());

        let mut bad_tag = DrcPaymentTx::unsigned_v4(
            source.address(),
            owner.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            1,
            Some(100),
        );
        sign_drc_payment_bound(&mut bad_tag, &source, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment(&store, &bad_tag, &ctx, &mut batch, &mut journal).is_err());

        let mut bad_source = DrcPaymentTx::unsigned_v4(
            payer.address(),
            owner.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            Some(42),
            None,
            Hash::ZERO,
            0,
            Some(100),
        );
        sign_drc_payment_bound(&mut bad_source, &payer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment(&store, &bad_source, &ctx, &mut batch, &mut journal).is_err());

        let mut expired = DrcPaymentTx::unsigned_v4(
            source.address(),
            owner.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            Some(42),
            None,
            Hash::ZERO,
            1,
            Some(10),
        );
        sign_drc_payment_bound(&mut expired, &source, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(apply_block_batched_with_auth_at_blue_score(
            &store,
            &{
                let mut b = coinbase_block(Hash::ZERO, 10);
                b.drc_payments = vec![expired];
                b.header.tx_root = b.compute_body_root();
                b
            },
            50,
            Some(&ctx),
            50,
        )
        .is_err());

        let mut master_pay = DrcPaymentTx::unsigned_v4(
            owner.address(),
            payer.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            5,
            None,
        );
        sign_drc_payment_bound(&mut master_pay, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(verify_drc_payment_operation(&store, &master_pay, &ctx).is_err());
    }

    #[test]
    fn same_block_disable_then_multisign_preauth_and_payment() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer = key(2);
        let source = key(3);
        fund(&store, &master, 300);
        fund(&store, &source, 100);
        install_list(&store, &master, &signer, 0);
        let ctx = auth();
        let owner = master.address();

        let mut disable =
            DrcAccountPolicyTx::set_master_key_disabled(owner, Amount::from_base_units(1), 1);
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut grant = DrcDepositPreauthTx::authorize(owner, source.address(), Amount::ZERO, 2);
        grant.public_key.clear();
        grant.signature.clear();
        grant.multisign = Some(multisign_auth(
            owner,
            &grant.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));

        let mut pay = DrcPaymentTx::unsigned(
            source.address(),
            owner,
            Amount::from_base_units(5),
            Amount::ZERO,
            0,
            Hash::ZERO,
            0,
        );
        sign_drc_payment_bound(&mut pay, &source, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut block = coinbase_block(Hash::ZERO, 7);
        block.drc_account_policies = vec![disable];
        block.drc_deposit_preauths = vec![grant];
        block.drc_payments = vec![pay];
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();

        let result = apply_block_batched_with_auth(&store, &block, 50, Some(&ctx)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(drc_master_key_disabled(&store, &owner).unwrap());
        assert_drc_recovery_invariant(&store, &owner).unwrap();
        assert!(load_drc_deposit_preauth(&store, &owner, &source.address()).unwrap());
    }

    #[test]
    fn cannot_remove_last_recovery_while_disabled() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer = key(2);
        fund(&store, &master, 100);
        install_list(&store, &master, &signer, 0);
        disable_master(&store, &master, 1);
        let ctx = auth();
        let owner = master.address();
        let before = snap(&store, &owner);

        let mut del_list = DrcSignerListTx::unsigned_delete(owner, Amount::ZERO, 2);
        del_list.public_key.clear();
        del_list.signature.clear();
        del_list.multisign = Some(multisign_auth(
            owner,
            &del_list.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        assert_apply_atomic_fail(&store, &owner, &before, |batch, journal| {
            apply_drc_signer_list(&store, &del_list, &ctx, batch, journal)
        });
    }

    #[test]
    fn replace_one_recovery_while_other_remains() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular_a = key(2);
        let regular_b = key(3);
        let signer = key(4);
        setup_disabled_with_both(&store, &master, &regular_a, &signer);
        let ctx = auth();
        let owner = master.address();

        let mut rk = DrcRegularKeyTx::set(
            owner,
            regular_b.address(),
            regular_b.public_key_bytes().to_vec(),
            Amount::ZERO,
            3,
        );
        sign_drc_regular_key_bound(&mut rk, &regular_a, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(&store, &rk, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_drc_recovery_invariant(&store, &owner).unwrap();

        let peer = key(8);
        let entries = agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
            signer: peer.address(),
            weight: 1,
        }]);
        let mut sl = DrcSignerListTx::unsigned_set(owner, entries, 1, Amount::ZERO, 4);
        sl.public_key.clear();
        sl.signature.clear();
        sl.multisign = Some(multisign_auth(
            owner,
            &sl.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(&store, &sl, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_drc_recovery_invariant(&store, &owner).unwrap();
    }

    #[test]
    fn same_block_transfer_lane_before_policy_rejects_master_transfer() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular = key(2);
        setup_disabled_with_both(&store, &master, &regular, &key(3));
        let ctx = auth();

        let mut transfer = AccountTransfer::unsigned_with_fee(
            NativeAssetId::DRC,
            master.address(),
            key(8).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            3,
        );
        sign_account_transfer_bound(&mut transfer, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut clear =
            DrcAccountPolicyTx::clear_master_key_disabled(master.address(), Amount::ZERO, 3);
        sign_drc_account_policy_bound(&mut clear, &regular, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut block = coinbase_block(Hash::ZERO, 8);
        block.account_transfers = vec![transfer];
        block.drc_account_policies = vec![clear];
        block.header.tx_root = block.compute_body_root();
        assert!(apply_block_batched_with_auth(&store, &block, 50, Some(&ctx)).is_err());
        assert!(drc_master_key_disabled(&store, &master.address()).unwrap());
    }

    #[test]
    fn recovery_invariant_bounded_property_sequence() {
        #[derive(Clone, Copy)]
        enum Step {
            SetRegularA,
            SetList,
            Disable,
            ReplaceRegularB,
            ReplaceList,
            AttemptDeleteListOnlyRecovery,
        }

        let store = StateStore::open_in_memory();
        let master = key(1);
        let regular_a = key(2);
        let regular_b = key(3);
        let signer = key(4);
        let peer = key(5);
        fund(&store, &master, 800);
        let owner = master.address();
        let ctx = auth();

        let sequence = [
            Step::SetRegularA,
            Step::SetList,
            Step::Disable,
            Step::ReplaceRegularB,
            Step::ReplaceList,
            Step::AttemptDeleteListOnlyRecovery,
        ];

        let mut nonce = 0u64;
        for step in sequence {
            match step {
                Step::SetRegularA => {
                    install_regular(&store, &master, &regular_a, nonce);
                    nonce += 1;
                }
                Step::SetList => {
                    install_list(&store, &master, &signer, nonce);
                    nonce += 1;
                }
                Step::Disable => {
                    disable_master(&store, &master, nonce);
                    nonce += 1;
                }
                Step::ReplaceRegularB => {
                    let mut tx = DrcRegularKeyTx::set(
                        owner,
                        regular_b.address(),
                        regular_b.public_key_bytes().to_vec(),
                        Amount::ZERO,
                        nonce,
                    );
                    sign_drc_regular_key_bound(&mut tx, &regular_a, &ctx.chain_id, &ctx.genesis)
                        .unwrap();
                    let mut batch = WriteBatch::new();
                    let mut journal = AccountJournal::default();
                    apply_drc_regular_key(&store, &tx, &ctx, &mut batch, &mut journal).unwrap();
                    store.write_batch(batch).unwrap();
                    nonce += 1;
                }
                Step::ReplaceList => {
                    let entries = agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
                        signer: peer.address(),
                        weight: 1,
                    }]);
                    let mut tx =
                        DrcSignerListTx::unsigned_set(owner, entries, 1, Amount::ZERO, nonce);
                    tx.public_key.clear();
                    tx.signature.clear();
                    tx.multisign = Some(multisign_auth(
                        owner,
                        &tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
                        &[&signer],
                        &ctx,
                    ));
                    let mut batch = WriteBatch::new();
                    let mut journal = AccountJournal::default();
                    apply_drc_signer_list(&store, &tx, &ctx, &mut batch, &mut journal).unwrap();
                    store.write_batch(batch).unwrap();
                    nonce += 1;
                }
                Step::AttemptDeleteListOnlyRecovery => {
                    let list_only = StateStore::open_in_memory();
                    let m = key(11);
                    let s = key(12);
                    fund(&list_only, &m, 200);
                    install_list(&list_only, &m, &s, 0);
                    disable_master(&list_only, &m, 1);
                    let mut tx = DrcSignerListTx::unsigned_delete(m.address(), Amount::ZERO, 2);
                    tx.public_key.clear();
                    tx.signature.clear();
                    tx.multisign = Some(multisign_auth(
                        m.address(),
                        &tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
                        &[&s],
                        &ctx,
                    ));
                    let mut batch = WriteBatch::new();
                    let mut journal = AccountJournal::default();
                    assert!(
                        apply_drc_signer_list(&list_only, &tx, &ctx, &mut batch, &mut journal)
                            .is_err()
                    );
                    assert!(batch.is_empty());
                }
            }
            if drc_master_key_disabled(&store, &owner).unwrap() {
                assert_drc_recovery_invariant(&store, &owner).unwrap();
            }
        }
    }

    #[test]
    fn clear_master_policy_tampered_multisign_attachment_rejected() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer = key(2);
        fund(&store, &master, 100);
        install_list(&store, &master, &signer, 0);
        disable_master(&store, &master, 1);
        let ctx = auth();

        let mut clear =
            DrcAccountPolicyTx::clear_master_key_disabled(master.address(), Amount::ZERO, 2);
        clear.public_key.clear();
        clear.signature.clear();
        clear.multisign = Some(multisign_auth(
            master.address(),
            &clear.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[&signer],
            &ctx,
        ));
        let mut block = coinbase_block(Hash::ZERO, 9);
        block.drc_account_policies = vec![clear];
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.drc_multisign_attachments[0].auth.signatures[0].signature = vec![0u8; 64];
        block.header.tx_root = block.compute_body_root();
        assert!(apply_block_batched_with_auth(&store, &block, 50, Some(&ctx)).is_err());
    }
}
