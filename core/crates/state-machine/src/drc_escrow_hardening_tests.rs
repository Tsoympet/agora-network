//! Escrow hardening: cutoff matrix, submitters, same-block lanes, policy, accounting, reorg.

#[cfg(test)]
mod cutoff_matrix {
    use agora_types::{
        escrow_cancel_allowed, escrow_finish_allowed, validate_escrow_time_bounds, Hash,
        TransactionAcceptance, DRC_ESCROW_MAX_BLUE_SCORE_BOUND,
    };

    use crate::apply::apply_block_batched_virtual_at_blue_score;
    use crate::drc_escrow::{apply_drc_escrow_finish, load_drc_escrow_receipt};
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, create_live_escrow, fund, key, load_block_acceptance,
        signed_cancel, signed_finish,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn pure_functions_no_wall_clock_dependency() {
        assert!(validate_escrow_time_bounds(None, Some(1)).is_ok());
        assert!(validate_escrow_time_bounds(Some(1), None).is_ok());
        assert!(validate_escrow_time_bounds(Some(DRC_ESCROW_MAX_BLUE_SCORE_BOUND), None).is_ok());
        assert!(
            validate_escrow_time_bounds(None, Some(DRC_ESCROW_MAX_BLUE_SCORE_BOUND + 1)).is_err()
        );
        assert!(!escrow_finish_allowed(9, Some(10), Some(20)));
        assert!(escrow_finish_allowed(10, Some(10), Some(20)));
        assert!(!escrow_finish_allowed(20, Some(10), Some(20)));
        assert!(!escrow_cancel_allowed(19, Some(20)));
        assert!(escrow_cancel_allowed(20, Some(20)));
        assert!(escrow_finish_allowed(5, None, Some(10)));
        assert!(!escrow_finish_allowed(10, None, Some(10)));
    }

    #[test]
    fn finish_cancel_window_at_canonical_scores() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let recipient = key(2);
        fund(&store, &owner, 10_000);
        let ctx = auth();
        let (id, _create) =
            create_live_escrow(&store, &owner, &recipient, 100, Some(10), Some(20), 1, &ctx);

        let finish_early = signed_finish(&owner, id, 1, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(
            apply_drc_escrow_finish(&store, &finish_early, &ctx, 9, &mut batch, &mut journal)
                .is_err()
        );

        let finish_ok = signed_finish(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_finishes.push(finish_ok);
        let block_id = apply_escrow_block(&store, block, 10, &ctx);
        let acc = load_block_acceptance(&store, &block_id);
        assert_eq!(
            acc.drc_escrow_finish_statuses[0],
            TransactionAcceptance::Accepted
        );

        let store2 = StateStore::open_in_memory();
        fund(&store2, &owner, 10_000);
        let (id2, _) =
            create_live_escrow(&store2, &owner, &recipient, 50, Some(10), Some(20), 1, &ctx);
        let finish_at_cancel = signed_finish(&owner, id2, 1, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_finishes.push(finish_at_cancel);
        block2.header.tx_root = block2.compute_body_root();
        let virtual_apply =
            apply_block_batched_virtual_at_blue_score(&store2, &block2, 50, Some(&ctx), 20)
                .unwrap();
        assert_eq!(
            virtual_apply.acceptance.drc_escrow_finish_statuses[0],
            TransactionAcceptance::ConflictLost
        );

        let cancel_ok = signed_cancel(&owner, id2, 1, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_escrow_cancels.push(cancel_ok);
        apply_escrow_block(&store2, block3, 20, &ctx);
        assert!(load_drc_escrow_receipt(&store2, &id2).unwrap().is_some());
    }
}

#[cfg(test)]
mod submitter_authorization {
    use agora_types::{Hash, NativeAssetId};

    use crate::accounts::load_account;
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, create_live_escrow, fund, key, signed_cancel,
        signed_finish,
    };
    use crate::StateStore;

    #[test]
    fn third_party_finish_and_cancel_release_to_owner_only() {
        let store = StateStore::open_in_memory();
        let owner = key(10);
        let recipient = key(11);
        let third = key(12);
        fund(&store, &owner, 5_000);
        fund(&store, &third, 500);
        let ctx = auth();
        let (id, _create) =
            create_live_escrow(&store, &owner, &recipient, 200, Some(5), Some(50), 1, &ctx);

        let finish = signed_finish(&third, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_finishes.push(finish);
        apply_escrow_block(&store, block, 5, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            200
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &third.address())
                .unwrap()
                .balance,
            500 - 1
        );

        let store2 = StateStore::open_in_memory();
        fund(&store2, &owner, 5_000);
        fund(&store2, &recipient, 100);
        let (id2, create2) =
            create_live_escrow(&store2, &owner, &recipient, 150, None, Some(30), 1, &ctx);
        let owner_after_create = load_account(&store2, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .balance;
        let cancel = signed_cancel(&recipient, id2, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_cancels.push(cancel);
        apply_escrow_block(&store2, block2, 30, &ctx);
        assert_eq!(
            load_account(&store2, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after_create + create2.amount.as_base_units()
        );
        assert_eq!(
            load_account(&store2, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            100 - 1
        );
    }
}

#[cfg(test)]
mod same_block_lane {
    use agora_types::{Hash, TransactionAcceptance};

    use crate::accounts::load_account;
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, fund, key, load_block_acceptance, signed_create,
        signed_finish,
    };
    use crate::StateStore;
    use agora_types::NativeAssetId;

    #[test]
    fn create_then_finish_in_one_block_when_eligible() {
        let store = StateStore::open_in_memory();
        let owner = key(20);
        let recipient = key(21);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            80,
            None,
            Some(100),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let finish = signed_finish(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        block.drc_escrow_finishes.push(finish);
        let block_id = apply_escrow_block(&store, block, 1, &ctx);
        let acc = load_block_acceptance(&store, &block_id);
        assert_eq!(
            acc.drc_escrow_create_statuses[0],
            TransactionAcceptance::Accepted
        );
        assert_eq!(
            acc.drc_escrow_finish_statuses[0],
            TransactionAcceptance::Accepted
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            80
        );
    }
}

#[cfg(test)]
mod policy_timing {
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, fund, key, signed_create,
    };
    use crate::drc_policy::apply_drc_account_policy;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_crypto::sign_drc_account_policy_bound;
    use agora_types::{DrcAccountPolicyTx, Hash};

    #[test]
    fn require_dest_tag_at_create_including_zero_tag() {
        let store = StateStore::open_in_memory();
        let owner = key(30);
        let recipient = key(31);
        fund(&store, &owner, 1_000);
        let ctx = auth();
        let mut pol = DrcAccountPolicyTx::set_require_destination_tag(
            recipient.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let no_tag = signed_create(
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
            &no_tag,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());

        let zero_tag = signed_create(
            &owner,
            recipient.address(),
            10,
            None,
            Some(50),
            0,
            &ctx,
            Some(0),
        );
        let mut block0 = coinbase(vec![Hash::ZERO], &owner);
        block0.drc_escrow_creates.push(zero_tag);
        apply_escrow_block(&store, block0, 1, &ctx);

        let owner_nonce = crate::accounts::load_account(
            &store,
            agora_types::NativeAssetId::DRC,
            &owner.address(),
        )
        .unwrap()
        .nonce;
        let tagged = signed_create(
            &owner,
            recipient.address(),
            10,
            None,
            Some(50),
            owner_nonce,
            &ctx,
            Some(7),
        );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(tagged);
        apply_escrow_block(&store, block, 1, &ctx);
    }
}

#[cfg(test)]
mod reorg_journal {
    use agora_types::Hash;

    use crate::apply::{apply_block_batched_with_auth_at_blue_score, revert_journal_batched};
    use crate::drc_escrow::load_drc_escrow_live;
    use crate::drc_escrow_test_harness::support::TIP;
    use crate::drc_escrow_test_harness::support::{auth, coinbase, fund, key, signed_create};
    use crate::state_root::compose_trident_state_root;
    use crate::StateStore;

    #[test]
    fn revert_restores_live_escrow_and_balances() {
        let store = StateStore::open_in_memory();
        let owner = key(40);
        let recipient = key(41);
        fund(&store, &owner, 3_000);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            120,
            None,
            Some(99),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        let root_before = compose_trident_state_root(&store, &TIP).unwrap();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 1).unwrap();
        store.write_batch(result.batch.clone()).unwrap();
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_some());
        store
            .write_batch(revert_journal_batched(&result.journal).unwrap())
            .unwrap();
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_none());
        assert_eq!(
            compose_trident_state_root(&store, &TIP).unwrap(),
            root_before
        );
    }
}

#[cfg(feature = "rocksdb")]
#[cfg(test)]
mod rocksdb_reopen {
    use agora_types::Hash;

    use crate::drc_escrow::{
        load_drc_escrow_live, load_drc_escrow_receipt, lookup_drc_escrow_point,
    };
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, fund, key, signed_cancel, signed_create,
    };
    use crate::StateStore;

    #[test]
    fn live_and_receipt_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path()).unwrap();
        let owner = key(50);
        let recipient = key(51);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            90,
            None,
            Some(10),
            0,
            &ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        apply_escrow_block(&store, block, 1, &ctx);
        drop(store);

        let reopened = StateStore::open(dir.path()).unwrap();
        assert_eq!(lookup_drc_escrow_point(&reopened, &id).unwrap(), "live");
        assert!(load_drc_escrow_live(&reopened, &id).unwrap().is_some());

        let cancel = signed_cancel(&owner, id, 1, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_escrow_cancels.push(cancel);
        apply_escrow_block(&reopened, block2, 10, &ctx);
        assert!(load_drc_escrow_receipt(&reopened, &id).unwrap().is_some());
        drop(reopened);

        let final_store = StateStore::open(dir.path()).unwrap();
        assert!(load_drc_escrow_receipt(&final_store, &id)
            .unwrap()
            .is_some());
    }
}
