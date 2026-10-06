//! AUTH-CUTOFF tranche: dest-tag/invoice, multisign adversaries, tickets, cutoffs, policy, conservation.

#[cfg(test)]
mod dest_tag_and_invoice {

    use agora_types::{DrcEscrowCreateTx, DrcEscrowError, Hash, DRC_ESCROW_CREATE_TX_VERSION};
    use borsh::BorshDeserialize;

    use crate::accounts::load_account;
    use crate::drc_escrow::{load_drc_escrow_live, load_drc_escrow_receipt};
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, fund, key, signed_create, signed_finish,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    #[test]
    fn some_zero_tag_satisfies_require_dest_tag_and_round_trips() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let recipient = key(2);
        fund(&store, &owner, 5_000);
        fund(&store, &recipient, 100);
        let ctx = auth();
        let mut pol = agora_types::DrcAccountPolicyTx::set_require_destination_tag(
            recipient.address(),
            agora_types::Amount::ZERO,
            0,
        );
        agora_crypto::sign_drc_account_policy_bound(
            &mut pol,
            &recipient,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();

        let create = signed_create(
            &owner,
            recipient.address(),
            25,
            None,
            Some(50),
            0,
            &ctx,
            Some(0),
        );
        assert_eq!(create.authenticated_destination_tag(), Some(0));
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create.clone());
        apply_escrow_block(&store, block, 1, &ctx);
        let live = load_drc_escrow_live(&store, &id).unwrap().unwrap();
        assert_eq!(live.destination_tag, Some(0));

        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish);
        apply_escrow_block(&store, block2, 1, &ctx);
        let receipt = load_drc_escrow_receipt(&store, &id).unwrap().unwrap();
        assert_eq!(receipt.destination_tag, Some(0));

        let bytes = borsh::to_vec(&create).unwrap();
        let decoded = DrcEscrowCreateTx::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.destination_tag, Some(0));
    }

    #[test]
    fn nonzero_invoice_id_rejected_without_state_mutation() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        fund(&store, &owner, 1_000);
        let _ctx = auth();
        let before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let tx = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: owner.address(),
            recipient: key(4).address(),
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash([9; 32]),
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(20),
            nonce: before.nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(matches!(
            tx.validate_structure(),
            Err(DrcEscrowError::NonZeroInvoiceNotSupported)
        ));
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            before
        );
    }
}

#[cfg(test)]
mod cutoff_deterministic {
    use agora_types::{validate_escrow_time_bounds, Hash, DRC_ESCROW_MAX_BLUE_SCORE_BOUND};

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_virtual_at_blue_score;
    use crate::drc_escrow::{apply_drc_escrow_cancel, apply_drc_escrow_finish};
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, create_live_escrow, fund, key, signed_cancel,
        signed_create, signed_finish,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    fn apply_at(
        store: &StateStore,
        id: Hash,
        op: &str,
        score: u64,
        owner: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) -> bool {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        match op {
            "finish" => {
                let finish = signed_finish(owner, id, nonce, ctx);
                let mut b = WriteBatch::new();
                let mut j = AccountJournal::default();
                apply_drc_escrow_finish(store, &finish, ctx, score, &mut b, &mut j).is_ok()
            }
            "cancel" => {
                let cancel = signed_cancel(owner, id, nonce, ctx);
                let mut b = WriteBatch::new();
                let mut j = AccountJournal::default();
                apply_drc_escrow_cancel(store, &cancel, ctx, score, &mut b, &mut j).is_ok()
            }
            _ => panic!(),
        }
    }

    #[test]
    fn create_bound_validation_matrix() {
        assert!(validate_escrow_time_bounds(None, None).is_err());
        assert!(validate_escrow_time_bounds(Some(10), Some(10)).is_err());
        assert!(validate_escrow_time_bounds(Some(11), Some(10)).is_err());
        assert!(validate_escrow_time_bounds(Some(5), Some(10)).is_ok());
        assert!(validate_escrow_time_bounds(None, Some(DRC_ESCROW_MAX_BLUE_SCORE_BOUND)).is_ok());
        assert!(
            validate_escrow_time_bounds(None, Some(DRC_ESCROW_MAX_BLUE_SCORE_BOUND + 1)).is_err()
        );
    }

    #[test]
    fn finish_only_cancel_absent_immediately_finishable() {
        let store = StateStore::open_in_memory();
        let owner = key(10);
        let recipient = key(11);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &owner, &recipient, 10, None, Some(100), 1, &ctx);
        assert!(apply_at(&store, id, "finish", 0, &owner, &ctx));
    }

    #[test]
    fn cancel_only_finish_before_cancel_window() {
        let store = StateStore::open_in_memory();
        let owner = key(12);
        let recipient = key(13);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            10,
            None,
            Some(30),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        assert!(apply_at(&store, id, "finish", 29, &owner, &ctx));
        assert!(!apply_at(&store, id, "cancel", 29, &owner, &ctx));
    }

    #[test]
    fn finish_after_bound_inclusive_cancel_exclusive() {
        let store = StateStore::open_in_memory();
        let owner = key(14);
        let recipient = key(15);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) =
            create_live_escrow(&store, &owner, &recipient, 10, Some(20), Some(40), 1, &ctx);
        assert!(!apply_at(&store, id, "finish", 19, &owner, &ctx));
        assert!(apply_at(&store, id, "finish", 20, &owner, &ctx));
    }

    #[test]
    fn at_cancel_score_finish_rejects_cancel_succeeds() {
        let store = StateStore::open_in_memory();
        let owner = key(16);
        let recipient = key(17);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) =
            create_live_escrow(&store, &owner, &recipient, 10, Some(5), Some(25), 1, &ctx);
        assert!(!apply_at(&store, id, "finish", 25, &owner, &ctx));
        assert!(apply_at(&store, id, "cancel", 25, &owner, &ctx));
    }

    #[test]
    fn virtual_lane_finish_at_cancel_is_conflict_not_hard_fail() {
        let store = StateStore::open_in_memory();
        let owner = key(18);
        let recipient = key(19);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) =
            create_live_escrow(&store, &owner, &recipient, 10, Some(5), Some(25), 1, &ctx);
        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_finishes.push(finish);
        block.header.tx_root = block.compute_body_root();
        let v =
            apply_block_batched_virtual_at_blue_score(&store, &block, 50, Some(&ctx), 25).unwrap();
        use agora_types::TransactionAcceptance;
        assert_eq!(
            v.acceptance.drc_escrow_finish_statuses[0],
            TransactionAcceptance::ConflictLost
        );
    }

    #[test]
    fn cancel_one_before_bound_rejects_exact_and_after_succeed() {
        let store = StateStore::open_in_memory();
        let owner = key(20);
        let recipient = key(21);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &owner, &recipient, 10, None, Some(30), 1, &ctx);
        assert!(!apply_at(&store, id, "cancel", 29, &owner, &ctx));
        assert!(apply_at(&store, id, "cancel", 30, &owner, &ctx));
    }

    #[test]
    fn finish_bound_at_max_representable_blue_score() {
        use agora_types::DRC_ESCROW_MAX_BLUE_SCORE_BOUND;
        let store = StateStore::open_in_memory();
        let owner = key(22);
        let recipient = key(23);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(
            &store,
            &owner,
            &recipient,
            10,
            Some(DRC_ESCROW_MAX_BLUE_SCORE_BOUND - 1),
            Some(DRC_ESCROW_MAX_BLUE_SCORE_BOUND),
            1,
            &ctx,
        );
        assert!(!apply_at(
            &store,
            id,
            "finish",
            DRC_ESCROW_MAX_BLUE_SCORE_BOUND - 2,
            &owner,
            &ctx
        ));
        assert!(apply_at(
            &store,
            id,
            "finish",
            DRC_ESCROW_MAX_BLUE_SCORE_BOUND - 1,
            &owner,
            &ctx
        ));
    }
}

#[cfg(test)]
mod policy_composition {
    use agora_crypto::{sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound};
    use agora_types::{DrcAccountPolicyTx, DrcDepositPreauthTx, Hash, NativeAssetId};

    use crate::accounts::load_account;
    use crate::drc_escrow::{apply_drc_escrow_finish, load_drc_escrow_live};
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, assert_escrow_snapshot_unchanged, auth, coinbase, create_live_escrow,
        fund, key, signed_create, signed_finish, snapshot_escrow_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn deposit_auth_checked_with_escrow_owner_as_source_third_party_finisher() {
        let store = StateStore::open_in_memory();
        let owner = key(30);
        let recipient = key(31);
        let finisher = key(32);
        fund(&store, &owner, 3_000);
        fund(&store, &recipient, 1);
        fund(&store, &finisher, 100);
        let ctx = auth();
        let mut pol = DrcAccountPolicyTx::set_deposit_auth_required(
            recipient.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let (id, _) = create_live_escrow(&store, &owner, &recipient, 50, None, Some(99), 1, &ctx);
        let snap = snapshot_escrow_state(&store, &owner, &recipient);
        let finish = signed_finish(&finisher, id, 0, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(
            apply_drc_escrow_finish(&store, &finish, &ctx, 1, &mut batch, &mut journal).is_err()
        );
        assert_escrow_snapshot_unchanged(&store, &owner, &recipient, &snap);
        let recipient_nonce = load_account(&store, NativeAssetId::DRC, &recipient.address())
            .unwrap()
            .nonce;
        let mut pre = DrcDepositPreauthTx::authorize(
            recipient.address(),
            owner.address(),
            agora_types::Amount::ZERO,
            recipient_nonce,
        );
        sign_drc_deposit_preauth_bound(&mut pre, &recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_deposit_preauth::apply_drc_deposit_preauth(
            &store,
            &pre,
            &ctx,
            &mut batch,
            &mut journal,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        let finish2 = signed_finish(&finisher, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_finishes.push(finish2);
        apply_escrow_block(&store, block, 2, &ctx);
        let recipient_balance = load_account(&store, NativeAssetId::DRC, &recipient.address())
            .unwrap()
            .balance;
        assert_eq!(recipient_balance, 1 + 50);
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_none());
    }

    #[test]
    fn require_dest_tag_enabled_after_untagged_create_grandfathered() {
        let store = StateStore::open_in_memory();
        let owner = key(33);
        let recipient = key(34);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            10,
            None,
            Some(50),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        let mut pol = DrcAccountPolicyTx::set_require_destination_tag(
            recipient.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish);
        apply_escrow_block(&store, block2, 2, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            10
        );
    }

    #[test]
    fn require_dest_tag_rejects_untagged_create_when_policy_on() {
        let store = StateStore::open_in_memory();
        let owner = key(35);
        let recipient = key(36);
        fund(&store, &owner, 2_000);
        fund(&store, &recipient, 1);
        let ctx = auth();
        let mut pol = DrcAccountPolicyTx::set_require_destination_tag(
            recipient.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let snap = snapshot_escrow_state(&store, &owner, &recipient);
        let create = signed_create(
            &owner,
            recipient.address(),
            10,
            None,
            Some(50),
            0,
            &ctx,
            None,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_escrow_snapshot_unchanged(&store, &owner, &recipient, &snap);
    }
}

#[cfg(test)]
mod value_conservation {
    use agora_types::{Hash, NativeAssetId, DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT};

    use crate::accounts::load_account;
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, count_live_escrows, fund, key, signed_cancel,
        signed_create, signed_finish, spendable_plus_locked,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn cap_thirty_two_then_finish_and_cancel_admit_new() {
        let store = StateStore::open_in_memory();
        let owner = key(40);
        let recipient = key(41);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let baseline = spendable_plus_locked(&store, &owner, &recipient);
        let mut nonce = 0u64;
        let mut ids = Vec::new();
        for i in 0..DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT {
            let create = signed_create(
                &owner,
                recipient.address(),
                1,
                None,
                Some(200 + i as u64),
                nonce,
                &ctx,
                None,
            );
            ids.push(create.escrow_id());
            let mut block = coinbase(vec![Hash::ZERO], &owner);
            block.drc_escrow_creates.push(create);
            apply_escrow_block(&store, block, 1, &ctx);
            nonce += 1;
        }
        assert_eq!(count_live_escrows(&store, &owner.address()), 32);
        let fail = signed_create(
            &owner,
            recipient.address(),
            1,
            None,
            Some(9999),
            nonce,
            &ctx,
            None,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &fail,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());

        let cancel = signed_cancel(
            &owner,
            ids[0],
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            &ctx,
        );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_cancels.push(cancel);
        apply_escrow_block(&store, block, 200, &ctx);
        let create2 = signed_create(
            &owner,
            recipient.address(),
            1,
            None,
            Some(5000),
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            &ctx,
            None,
        );
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_creates.push(create2);
        apply_escrow_block(&store, block2, 1, &ctx);
        assert_eq!(count_live_escrows(&store, &owner.address()), 32);
        assert!(spendable_plus_locked(&store, &owner, &recipient) >= baseline - 64);
    }

    #[test]
    fn finish_debits_submitter_fee_only_recipient_gets_amount() {
        let store = StateStore::open_in_memory();
        let owner = key(42);
        let recipient = key(43);
        let helper = key(44);
        fund(&store, &owner, 500);
        fund(&store, &helper, 20);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            100,
            None,
            Some(50),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        let owner_after = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .balance;
        let finish = signed_finish(&helper, id, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish);
        apply_escrow_block(&store, block2, 1, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            100
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &helper.address())
                .unwrap()
                .balance,
            19
        );
    }

    #[test]
    fn cancel_returns_exact_lock_to_owner_submitter_pays_fee() {
        let store = StateStore::open_in_memory();
        let owner = key(45);
        let recipient = key(46);
        let helper = key(47);
        fund(&store, &owner, 500);
        fund(&store, &helper, 20);
        let ctx = auth();
        let owner_before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let create = signed_create(
            &owner,
            recipient.address(),
            100,
            None,
            Some(50),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        let owner_after_create =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(owner_after_create.balance, owner_before.balance - 101);
        let cancel = signed_cancel(&helper, id, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_cancels.push(cancel);
        apply_escrow_block(&store, block2, 50, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after_create.balance + 100
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &helper.address())
                .unwrap()
                .balance,
            19
        );
    }
}

#[cfg(test)]
mod multisign_adversary {
    use agora_types::DrcMultisignOperationKind;

    use crate::accounts::load_account;
    use crate::drc_escrow_test_harness::multisign::{
        self, base_multisign_cancel_block, base_multisign_create_block,
        base_multisign_finish_block, install_signer_list, multisign_cancel_block,
        multisign_finish_block, reject_preserving,
    };
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, assert_escrow_snapshot_unchanged, auth, create_live_escrow, fund, key,
        snapshot_escrow_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    macro_rules! escrow_create_multisign_case {
        ($name:ident, $tamper:expr) => {
            #[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(50);
                let recipient = key(51);
                let s1 = key(52);
                let s2 = key(53);
                fund(&store, &master, 100_000);
                let ctx = auth();
                let signers: &[(&agora_crypto::KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                let mut block =
                    base_multisign_create_block(&store, &master, &recipient, signers, &ctx);
                let before = snapshot_escrow_state(&store, &master, &recipient);
                let tamper: fn(
                    &mut agora_types::Block,
                    &agora_crypto::KeyPair,
                    &[(&agora_crypto::KeyPair, u16)],
                    &crate::apply::TxAuthContext,
                ) = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &recipient, block, &ctx, &before);
            }
        };
    }

    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_missing_attachment,
        |block, _, _, _| { block.drc_multisign_attachments.clear() }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_duplicate_attachment,
        |block, _, _, _| {
            block
                .drc_multisign_attachments
                .push(block.drc_multisign_attachments[0].clone())
        }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_orphan_attachment,
        |block, _, _, _| { block.drc_escrow_creates.clear() }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_wrong_kind,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcTicketCreate
        }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_mixed_single_and_multisign,
        |block, master, _, _| {
            block.drc_escrow_creates[0].public_key = master.public_key_bytes().to_vec();
            block.drc_escrow_creates[0].signature = vec![1; 64];
        }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_tampered_signature,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff
        }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_below_quorum,
        |block, master, _, ctx| {
            let signing =
                block.drc_escrow_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth =
                multisign::multisign_bundle(master.address(), &signing, &[(&key(88), 1)], ctx);
        }
    );
    escrow_create_multisign_case!(
        escrow_create_multisign_rejects_foreign_signer,
        |block, master, _, ctx| {
            let signing =
                block.drc_escrow_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
                master.address(),
                &signing,
                &[(&key(90), 1), (&key(91), 2)],
                ctx,
            );
        }
    );

    #[test]
    fn escrow_create_multisign_positive_apply_borsh_body_root() {
        let store = StateStore::open_in_memory();
        let master = key(60);
        let recipient = key(61);
        let s1 = key(62);
        let s2 = key(63);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let signers: &[(&agora_crypto::KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
        let mut block = base_multisign_create_block(&store, &master, &recipient, signers, &ctx);
        let root = block.compute_body_root();
        block.header.tx_root = root;
        apply_escrow_block(&store, block, 1, &ctx);
    }

    #[test]
    fn escrow_finish_multisign_positive_after_live_create() {
        let store = StateStore::open_in_memory();
        let master = key(64);
        let recipient = key(65);
        let s1 = key(66);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 15, None, Some(80), 1, &ctx);
        let signers: &[(&agora_crypto::KeyPair, u16)] = &[(&s1, 1)];
        let block = base_multisign_finish_block(&store, &master, &recipient, id, signers, &ctx);
        apply_escrow_block(&store, block, 2, &ctx);
    }

    #[test]
    fn escrow_cancel_multisign_positive_third_party_submitter() {
        let store = StateStore::open_in_memory();
        let master = key(67);
        let helper = key(68);
        let s1 = key(69);
        let recipient = key(70);
        fund(&store, &master, 50_000);
        fund(&store, &helper, 500);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 20, None, Some(40), 1, &ctx);
        let signers: &[(&agora_crypto::KeyPair, u16)] = &[(&s1, 1)];
        let block =
            base_multisign_cancel_block(&store, &helper, &master, &recipient, id, signers, &ctx);
        apply_escrow_block(&store, block, 40, &ctx);
    }

    macro_rules! escrow_finish_multisign_case {
        ($name:ident, $tamper:expr) => {
            #[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(74);
                let recipient = key(75);
                let s1 = key(76);
                let s2 = key(77);
                fund(&store, &master, 100_000);
                let ctx = auth();
                let (id, _) =
                    create_live_escrow(&store, &master, &recipient, 12, None, Some(90), 1, &ctx);
                let signers: &[(&agora_crypto::KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                install_signer_list(&store, &master, signers, &ctx);
                let before = snapshot_escrow_state(&store, &master, &recipient);
                let mut block = multisign_finish_block(&store, &master, id, signers, &ctx);
                let tamper: fn(
                    &mut agora_types::Block,
                    &agora_crypto::KeyPair,
                    &[(&agora_crypto::KeyPair, u16)],
                    &crate::apply::TxAuthContext,
                ) = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &recipient, block, &ctx, &before);
            }
        };
    }

    escrow_finish_multisign_case!(
        escrow_finish_multisign_rejects_wrong_signing_for,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].auth.signing_for = key(99).address();
        }
    );
    escrow_finish_multisign_case!(
        escrow_finish_multisign_rejects_missing_attachment,
        |block, _, _, _| {
            block.drc_multisign_attachments.clear();
        }
    );
    escrow_finish_multisign_case!(
        escrow_finish_multisign_rejects_below_quorum,
        |block, master, _, ctx| {
            let signing =
                block.drc_escrow_finishes[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth =
                multisign::multisign_bundle(master.address(), &signing, &[(&key(100), 1)], ctx);
        }
    );

    macro_rules! escrow_cancel_multisign_case {
        ($name:ident, $tamper:expr) => {
            #[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(101);
                let helper = key(102);
                let recipient = key(103);
                let s1 = key(104);
                fund(&store, &master, 100_000);
                fund(&store, &helper, 500);
                let ctx = auth();
                let (id, _) =
                    create_live_escrow(&store, &master, &recipient, 12, None, Some(60), 1, &ctx);
                let signers: &[(&agora_crypto::KeyPair, u16)] = &[(&s1, 1)];
                install_signer_list(&store, &helper, signers, &ctx);
                let before = snapshot_escrow_state(&store, &master, &recipient);
                let mut block = multisign_cancel_block(&store, &helper, &master, id, signers, &ctx);
                let tamper: fn(
                    &mut agora_types::Block,
                    &agora_crypto::KeyPair,
                    &[(&agora_crypto::KeyPair, u16)],
                    &crate::apply::TxAuthContext,
                ) = $tamper;
                tamper(&mut block, &helper, signers, &ctx);
                reject_preserving(&store, &master, &recipient, block, &ctx, &before);
            }
        };
    }

    escrow_cancel_multisign_case!(
        escrow_cancel_multisign_rejects_wrong_kind,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].key.kind =
                DrcMultisignOperationKind::DrcEscrowCreate;
        }
    );
    escrow_cancel_multisign_case!(
        escrow_cancel_multisign_rejects_tampered_signature,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
        }
    );

    #[test]
    fn disabled_master_escrow_create_rejected_with_regular_key_recovery() {
        use agora_crypto::{
            sign_drc_account_policy_bound, sign_drc_escrow_create_bound, sign_drc_regular_key_bound,
        };
        use agora_types::DrcAccountPolicyTx;
        use agora_types::DrcRegularKeyTx;
        let store = StateStore::open_in_memory();
        let master = key(71);
        let regular = key(72);
        let recipient = key(73);
        fund(&store, &master, 100_000);
        let ctx = auth();
        let mut reg = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_regular_key_bound(&mut reg, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_regular_key::apply_drc_regular_key(&store, &reg, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            agora_types::Amount::ZERO,
            1,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(
            &store,
            &disable,
            &ctx,
            &mut batch,
            &mut journal,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        let before = snapshot_escrow_state(&store, &master, &recipient);
        let nonce = load_account(&store, agora_types::NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = crate::drc_escrow_test_harness::support::signed_create(
            &master,
            recipient.address(),
            5,
            None,
            Some(50),
            nonce,
            &ctx,
            None,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_escrow_snapshot_unchanged(&store, &master, &recipient, &before);
        sign_drc_escrow_create_bound(&mut create, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_ok());
    }
}

#[cfg(test)]
mod ticket_matrix {
    use agora_crypto::{
        sign_drc_escrow_create_bound, sign_drc_escrow_finish_bound, sign_drc_ticket_create_bound,
    };
    use agora_types::{
        DrcAccountSequenceSelector, DrcTicketCreateTx, Hash, DRC_ESCROW_CREATE_TICKET_VERSION,
        DRC_ESCROW_FINISH_TICKET_VERSION,
    };

    use crate::apply::apply_block_batched_with_auth_at_blue_score;
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, assert_escrow_snapshot_unchanged, auth, coinbase, fund, key,
        snapshot_escrow_state,
    };
    use crate::drc_mempool::lookup_drc_ticket_point;
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn mint_ticket(
        store: &StateStore,
        owner: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) -> u64 {
        let nonce =
            crate::accounts::load_account(store, agora_types::NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce;
        let mut create =
            DrcTicketCreateTx::unsigned(owner.address(), agora_types::Amount::ZERO, nonce);
        sign_drc_ticket_create_bound(&mut create, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        store
            .write_batch(
                apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), 50)
                    .unwrap()
                    .batch,
            )
            .unwrap();
        nonce + 1
    }

    #[test]
    fn ticket_escrow_create_finish_cancel_one_use_each() {
        let store = StateStore::open_in_memory();
        let owner = key(80);
        let recipient = key(81);
        fund(&store, &owner, 500_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx);
        let mut create = agora_types::DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner: owner.address(),
            recipient: recipient.address(),
            amount: agora_types::Amount::from_base_units(10),
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
        sign_drc_escrow_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        assert!(!load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq));
        let seq2 = mint_ticket(&store, &owner, &ctx);
        let mut finish = agora_types::DrcEscrowFinishTx {
            version: DRC_ESCROW_FINISH_TICKET_VERSION,
            submitter: owner.address(),
            escrow_id: id,
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq2)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_finish_bound(&mut finish, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish);
        apply_escrow_block(&store, block2, 1, &ctx);
    }

    #[test]
    fn unknown_ticket_rejected_create_preserves_tickets() {
        let store = StateStore::open_in_memory();
        let owner = key(82);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let before = snapshot_escrow_state(&store, &owner, &key(83));
        let mut create = agora_types::DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner: owner.address(),
            recipient: key(83).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(50),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(999)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_escrow_snapshot_unchanged(&store, &owner, &key(83), &before);
        assert_eq!(
            lookup_drc_ticket_point(&store, &owner.address(), 999).unwrap(),
            crate::drc_mempool::DrcTicketPointStatus::Unknown
        );
    }

    #[test]
    fn legacy_escrow_version_rejects_ticket_selector() {
        let store = StateStore::open_in_memory();
        let owner = key(84);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx);
        let create = agora_types::DrcEscrowCreateTx {
            version: agora_types::DRC_ESCROW_CREATE_TX_VERSION,
            owner: owner.address(),
            recipient: key(85).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(50),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(create.validate_structure().is_err());
    }

    #[test]
    fn reused_ticket_rejected_second_create_preserves_ticket_set() {
        let store = StateStore::open_in_memory();
        let owner = key(86);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx);
        let recipient = key(87);
        let mut create = agora_types::DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner: owner.address(),
            recipient: recipient.address(),
            amount: agora_types::Amount::from_base_units(2),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(80),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        let mut create2 = agora_types::DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner: owner.address(),
            recipient: key(88).address(),
            amount: agora_types::Amount::from_base_units(2),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(90),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut create2, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let snap = snapshot_escrow_state(&store, &owner, &recipient);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &create2,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_escrow_snapshot_unchanged(&store, &owner, &recipient, &snap);
        assert_eq!(
            lookup_drc_ticket_point(&store, &owner.address(), seq).unwrap(),
            crate::drc_mempool::DrcTicketPointStatus::Unknown
        );
    }

    #[test]
    fn cross_owner_ticket_sequence_rejected() {
        let store = StateStore::open_in_memory();
        let owner = key(89);
        let other = key(90);
        fund(&store, &owner, 50_000);
        fund(&store, &other, 50_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &other, &ctx);
        let mut create = agora_types::DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TICKET_VERSION,
            owner: owner.address(),
            recipient: key(91).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(40),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let snap = snapshot_escrow_state(&store, &owner, &key(91));
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_escrow::apply_drc_escrow_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_escrow_snapshot_unchanged(&store, &owner, &key(91), &snap);
        assert!(load_drc_account_tickets(&store, &other.address())
            .unwrap()
            .contains(&seq));
    }
}
