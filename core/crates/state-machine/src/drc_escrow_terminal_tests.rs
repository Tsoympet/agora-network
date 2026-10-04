//! FINAL persistence/public tranche: reorg, RocksDB, auth depth, invariants.

#[cfg(test)]
mod reorg_journal_transitions {
    use agora_types::{Hash, NativeAssetId, TransactionAcceptance};

    use crate::accounts::load_account;
    use crate::drc_escrow::{
        drc_escrow_root, load_drc_escrow_live, load_drc_escrow_receipt,
    };
    use crate::drc_escrow_test_harness::support::{
        apply_block_capture, auth, coinbase, count_live_escrows, create_live_escrow, fund, key,
        load_block_acceptance, locked_escrow_total, revert_journal, signed_cancel, signed_create,
        signed_finish, TIP,
    };
    
    use crate::state_root::compose_trident_state_root;
    use crate::StateStore;

    #[test]
    fn revert_create_restores_owner_nonce_balance_live_count_and_roots() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let recipient = key(2);
        fund(&store, &owner, 5_000);
        let ctx = auth();
        let before_bal = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let root_before = compose_trident_state_root(&store, &TIP).unwrap();
        let escrow_root_before = drc_escrow_root(&store).unwrap();
        let create = signed_create(&owner, recipient.address(), 100, None, Some(50), 0, &ctx, None);
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        let (_, journal, _) = apply_block_capture(&store, block, 1, &ctx);
        assert_eq!(count_live_escrows(&store, &owner.address()), 1);
        revert_journal(&store, &journal);
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_none());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            before_bal
        );
        assert_eq!(count_live_escrows(&store, &owner.address()), 0);
        assert_eq!(compose_trident_state_root(&store, &TIP).unwrap(), root_before);
        assert_eq!(drc_escrow_root(&store).unwrap(), escrow_root_before);
    }

    #[test]
    fn revert_finish_restores_live_escrow_recipient_balance_and_submitter_fee() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        let recipient = key(4);
        let helper = key(5);
        fund(&store, &owner, 3_000);
        fund(&store, &helper, 50);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &owner, &recipient, 80, None, Some(40), 1, &ctx);
        let locked_before = locked_escrow_total(&store, &owner.address());
        let recipient_before =
            load_account(&store, NativeAssetId::DRC, &recipient.address()).unwrap().balance;
        let helper_before =
            load_account(&store, NativeAssetId::DRC, &helper.address()).unwrap().balance;
        let finish = signed_finish(&helper, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_finishes.push(finish);
        let (_, journal, _) = apply_block_capture(&store, block, 2, &ctx);
        assert!(load_drc_escrow_receipt(&store, &id).unwrap().is_some());
        revert_journal(&store, &journal);
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_some());
        assert!(load_drc_escrow_receipt(&store, &id).unwrap().is_none());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address()).unwrap().balance,
            recipient_before
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &helper.address()).unwrap().balance,
            helper_before
        );
        assert_eq!(locked_escrow_total(&store, &owner.address()), locked_before);
    }

    #[test]
    fn revert_cancel_restores_live_escrow_and_owner_locked_value() {
        let store = StateStore::open_in_memory();
        let owner = key(6);
        let recipient = key(7);
        let helper = key(8);
        fund(&store, &owner, 2_000);
        fund(&store, &helper, 20);
        let ctx = auth();
        let (id, create) = create_live_escrow(&store, &owner, &recipient, 60, None, Some(30), 1, &ctx);
        let owner_after_create =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap().balance;
        let cancel = signed_cancel(&helper, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_cancels.push(cancel);
        let (_, journal, _) = apply_block_capture(&store, block, 30, &ctx);
        revert_journal(&store, &journal);
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_some());
        assert!(load_drc_escrow_receipt(&store, &id).unwrap().is_none());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap().balance,
            owner_after_create
        );
        assert_eq!(create.amount.as_base_units(), 60);
    }

    #[test]
    fn reapply_after_revert_is_deterministic_for_finish_block() {
        let store = StateStore::open_in_memory();
        let owner = key(9);
        let recipient = key(10);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &owner, &recipient, 25, None, Some(60), 1, &ctx);
        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_finishes.push(finish);
        let (block_id, journal, acceptance) = apply_block_capture(&store, block.clone(), 5, &ctx);
        let root1 = compose_trident_state_root(&store, &TIP).unwrap();
        revert_journal(&store, &journal);
        let (_, journal2, acceptance2) = apply_block_capture(&store, block, 5, &ctx);
        let root2 = compose_trident_state_root(&store, &TIP).unwrap();
        assert_eq!(root1, root2);
        assert_eq!(acceptance.drc_escrow_finish_statuses, acceptance2.drc_escrow_finish_statuses);
        assert_eq!(
            load_block_acceptance(&store, &block_id).drc_escrow_finish_statuses[0],
            TransactionAcceptance::Accepted
        );
        let _ = journal2;
    }
}

#[cfg(feature = "rocksdb")]
#[cfg(test)]
mod rocksdb_reopen_parity {
    use agora_types::{Hash, NativeAssetId};

    use crate::accounts::load_account;
    use crate::drc_escrow::{
        drc_escrow_root, load_drc_escrow_live, load_drc_escrow_receipt, lookup_drc_escrow_point,
    };
    use crate::drc_escrow_test_harness::support::{
        apply_block_capture, apply_escrow_block, auth, coinbase, count_live_escrows, fund, key,
        revert_journal, signed_cancel, signed_create, signed_finish,
    };
    use crate::StateStore;

    #[test]
    fn reopen_live_finish_cancel_and_rollback_paths() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path()).unwrap();
        let owner = key(20);
        let recipient = key(21);
        fund(&store, &owner, 10_000);
        let ctx = auth();
        let create = signed_create(&owner, recipient.address(), 40, None, Some(20), 0, &ctx, None);
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        drop(store);

        let reopened = StateStore::open(dir.path()).unwrap();
        assert_eq!(lookup_drc_escrow_point(&reopened, &id).unwrap(), "live");
        assert_eq!(count_live_escrows(&reopened, &owner.address()), 1);
        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish);
        apply_escrow_block(&reopened, block2, 2, &ctx);
        assert!(load_drc_escrow_receipt(&reopened, &id).unwrap().is_some());
        drop(reopened);

        let after_finish = StateStore::open(dir.path()).unwrap();
        assert_eq!(
            load_account(&after_finish, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            40
        );

        let store2 = StateStore::open_in_memory();
        fund(&store2, &owner, 10_000);
        let (id2, _) = crate::drc_escrow_test_harness::support::create_live_escrow(
            &store2, &owner, &recipient, 10, None, Some(15), 1, &ctx,
        );
        let cancel = signed_cancel(&owner, id2, 1, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_escrow_cancels.push(cancel);
        let (_, journal, _) = apply_block_capture(&store2, block3, 15, &ctx);
        revert_journal(&store2, &journal);
        assert!(load_drc_escrow_live(&store2, &id2).unwrap().is_some());
        assert_eq!(drc_escrow_root(&after_finish).unwrap(), drc_escrow_root(&after_finish).unwrap());
    }
}

#[cfg(test)]
mod auth_disabled_master_ticket_multisign {
    use agora_crypto::{
        sign_drc_account_policy_bound, sign_drc_escrow_create_bound, sign_drc_regular_key_bound, sign_drc_ticket_create_bound,
    };
    use agora_types::{
        DrcAccountPolicyTx, DrcAccountSequenceSelector, DrcEscrowCreateTx, DrcEscrowFinishTx, DrcRegularKeyTx, DrcTicketCreateTx, Hash, DRC_ESCROW_CREATE_TICKET_VERSION,
        DRC_ESCROW_FINISH_TICKET_VERSION,
    };

    use crate::accounts::load_account;
    use crate::drc_escrow_test_harness::multisign::{
        install_signer_list, multisign_bundle,
    };
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, fund, key, signed_create,
        snapshot_escrow_state, assert_escrow_snapshot_unchanged,
    };
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    fn install_regular_key(
        store: &StateStore,
        master: &agora_crypto::KeyPair,
        regular: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) {
        let nonce = load_account(store, NativeAssetId::DRC, &master.address()).unwrap().nonce;
        let mut reg = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            agora_types::Amount::ZERO,
            nonce,
        );
        sign_drc_regular_key_bound(&mut reg, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_regular_key::apply_drc_regular_key(store, &reg, ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn disable_master_with_regular(
        store: &StateStore,
        master: &agora_crypto::KeyPair,
        regular: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) {
        let reg_nonce = load_account(store, NativeAssetId::DRC, &master.address()).unwrap().nonce;
        let mut reg = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            agora_types::Amount::ZERO,
            reg_nonce,
        );
        sign_drc_regular_key_bound(&mut reg, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_regular_key::apply_drc_regular_key(store, &reg, ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let disable_nonce = load_account(store, NativeAssetId::DRC, &master.address()).unwrap().nonce;
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            agora_types::Amount::ZERO,
            disable_nonce,
        );
        sign_drc_account_policy_bound(&mut disable, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(store, &disable, ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn mint_ticket(
        store: &StateStore,
        owner: &agora_crypto::KeyPair,
        signer: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) -> u64 {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address()).unwrap().nonce;
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), agora_types::Amount::ZERO, nonce);
        sign_drc_ticket_create_bound(&mut create, signer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_ticket_creates.push(create);
        apply_escrow_block(store, block, 1, ctx);
        nonce + 1
    }

    #[test]
    fn ticket_create_rejected_with_disabled_master_regular_key_succeeds() {
        let store = StateStore::open_in_memory();
        let master = key(30);
        let regular = key(31);
        let recipient = key(32);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &master, &master, &ctx);
        disable_master_with_regular(&store, &master, &regular, &ctx);
        let snap = snapshot_escrow_state(&store, &master, &recipient);
        let mut create = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner: master.address(),
            recipient: recipient.address(),
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut create, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store, &create, &ctx, 1, &mut batch, &mut journal
        )
        .is_err());
        assert_escrow_snapshot_unchanged(&store, &master, &recipient, &snap);
        assert!(load_drc_account_tickets(&store, &master.address()).unwrap().contains(&seq));
        sign_drc_escrow_create_bound(&mut create, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &master);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        assert!(!load_drc_account_tickets(&store, &master.address()).unwrap().contains(&seq));
    }

    #[test]
    fn ticket_finish_and_cancel_multisign_positive_with_disabled_master() {
        let store = StateStore::open_in_memory();
        let master = key(33);
        let regular = key(34);
        let recipient = key(35);
        let s1 = key(36);
        fund(&store, &master, 80_000);
        let ctx = auth();
        install_regular_key(&store, &master, &regular, &ctx);
        install_signer_list(&store, &master, &[(&s1, 1)], &ctx);
        let disable_nonce = load_account(&store, NativeAssetId::DRC, &master.address()).unwrap().nonce;
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            agora_types::Amount::ZERO,
            disable_nonce,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &disable, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address()).unwrap().nonce;
        let mut create = signed_create(
            &master,
            recipient.address(),
            12,
            None,
            Some(50),
            nonce,
            &ctx,
            None,
        );
        sign_drc_escrow_create_bound(&mut create, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &master);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        let seq = mint_ticket(&store, &master, &regular, &ctx);
        let mut finish = DrcEscrowFinishTx {
            version: DRC_ESCROW_FINISH_TICKET_VERSION,
            submitter: master.address(),
            escrow_id: id,
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: Some(multisign_bundle(
                master.address(),
                &DrcEscrowFinishTx {
                    version: DRC_ESCROW_FINISH_TICKET_VERSION,
                    submitter: master.address(),
                    escrow_id: id,
                    fee: agora_types::Amount::from_base_units(1),
                    nonce: 0,
                    account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
                    public_key: Vec::new(),
                    signature: Vec::new(),
                    multisign: None,
                }
                .signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
                &[(&s1, 1)],
                &ctx,
            )),
        };
        finish.public_key.clear();
        finish.signature.clear();
        let mut block2 = coinbase(vec![Hash::ZERO], &master);
        block2.drc_escrow_finishes.push(finish);
        agora_types::materialize_drc_multisign_attachments(&mut block2, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        block2.header.tx_root = block2.compute_body_root();
        apply_escrow_block(&store, block2, 2, &ctx);
    }
}

#[cfg(test)]
mod multisign_adversary_terminal {
    use agora_types::{
        DrcMultisignOperationKind, Hash,
        DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::drc_escrow_test_harness::multisign::{
        install_signer_list, multisign_finish_block, reject_preserving,
    };
    use crate::drc_escrow_test_harness::support::{
        auth, create_live_escrow, fund, key, snapshot_escrow_state,
    };
    use crate::StateStore;

    #[test]
    fn create_multisign_rejects_wrong_signing_commitment_id() {
        let store = StateStore::open_in_memory();
        let master = key(40);
        let recipient = key(41);
        let s1 = key(42);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let signers = &[(&s1, 1)];
        install_signer_list(&store, &master, signers, &ctx);
        let mut block = crate::drc_escrow_test_harness::multisign::base_multisign_create_block(
            &store, &master, &recipient, signers, &ctx,
        );
        let before = snapshot_escrow_state(&store, &master, &recipient);
        block.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
        reject_preserving(&store, &master, &recipient, block, &ctx, &before);
    }

    #[test]
    fn finish_multisign_rejects_unsorted_attachment_lane() {
        let store = StateStore::open_in_memory();
        let master = key(43);
        let recipient = key(44);
        let s1 = key(45);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 10, None, Some(80), 1, &ctx);
        install_signer_list(&store, &master, &[(&s1, 1)], &ctx);
        let block = multisign_finish_block(&store, &master, id, &[(&s1, 1)], &ctx);
        let mut block2 = block;
        block2.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcEscrowCreate;
        let before = snapshot_escrow_state(&store, &master, &recipient);
        reject_preserving(&store, &master, &recipient, block2, &ctx, &before);
    }

    #[test]
    fn create_multisign_rejects_oversized_auth_entry_list() {
        let store = StateStore::open_in_memory();
        let master = key(46);
        let recipient = key(47);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let signers = &[(&key(48), 1)];
        let mut block = crate::drc_escrow_test_harness::multisign::base_multisign_create_block(
            &store, &master, &recipient, signers, &ctx,
        );
        let before = snapshot_escrow_state(&store, &master, &recipient);
        let sample = block.drc_multisign_attachments[0].auth.signatures[0].clone();
        block.drc_multisign_attachments[0]
            .auth
            .signatures
            .resize(DRC_MULTISIGN_MAX_SIGNATURES + 1, sample);
        reject_preserving(&store, &master, &recipient, block, &ctx, &before);
    }
}

#[cfg(test)]
mod deterministic_invariant_sequence {
    use agora_crypto::sign_drc_account_policy_bound;
    use agora_types::{
        DrcAccountPolicyTx, Hash, NativeAssetId, TransactionAcceptance,
    };

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_virtual_at_blue_score;
    use crate::drc_escrow::{load_drc_escrow_live, lookup_drc_escrow_point};
    use crate::drc_escrow_test_harness::support::{
        apply_block_capture, apply_escrow_block, auth, coinbase, fund, key, revert_journal,
        signed_cancel, signed_create, signed_finish, spendable_plus_locked,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn create_policy_reject_finish_cancel_conflict_rollback_conserves_aggregate() {
        let store = StateStore::open_in_memory();
        let owner = key(50);
        let recipient = key(51);
        fund(&store, &owner, 20_000);
        fund(&store, &recipient, 1);
        let ctx = auth();
        let baseline = spendable_plus_locked(&store, &owner, &recipient);
        let create = signed_create(&owner, recipient.address(), 50, None, Some(100), 0, &ctx, None);
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        let mut pol = DrcAccountPolicyTx::set_deposit_auth_required(
            recipient.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let bad_finish = signed_finish(&owner, id, 1, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_finish(
            &store, &bad_finish, &ctx, 2, &mut batch, &mut journal
        )
        .is_err());
        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish);
        block2.header.tx_root = block2.compute_body_root();
        let virtual_apply =
            apply_block_batched_virtual_at_blue_score(&store, &block2, 50, Some(&ctx), 5).unwrap();
        assert_eq!(
            virtual_apply.acceptance.drc_escrow_finish_statuses[0],
            TransactionAcceptance::ConflictLost
        );
        let cancel = signed_cancel(&owner, id, 1, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_escrow_cancels.push(cancel);
        let (_, journal3, _) = apply_block_capture(&store, block3, 100, &ctx);
        revert_journal(&store, &journal3);
        assert_eq!(lookup_drc_escrow_point(&store, &id).unwrap(), "live");
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_some());
        assert!(spendable_plus_locked(&store, &owner, &recipient) >= baseline - 2);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap().nonce,
            1
        );
    }
}
