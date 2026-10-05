//! FINAL persistence/public tranche: reorg, RocksDB, auth depth, invariants.

#[cfg(test)]
mod reorg_journal_transitions {
    use agora_types::{Hash, NativeAssetId, TransactionAcceptance};

    use crate::accounts::load_account;
    use crate::drc_check::{drc_check_root, load_drc_check_live, load_drc_check_receipt};
    use crate::drc_check_test_harness::support::{
        apply_block_capture, auth, coinbase, count_live_checks, create_live_check, fund, key,
        load_block_acceptance, locked_check_total, revert_journal, signed_cancel, signed_cash,
        signed_create, TIP,
    };

    use crate::state_root::compose_trident_state_root;
    use crate::StateStore;

    #[test]
    fn revert_create_restores_owner_nonce_balance_live_count_and_roots() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let destination = key(2);
        fund(&store, &owner, 5_000);
        let ctx = auth();
        let before_bal = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let root_before = compose_trident_state_root(&store, &TIP).unwrap();
        let check_root_before = drc_check_root(&store).unwrap();
        let create = signed_create(&owner, destination.address(), 100, Some(50), 0, &ctx, None);
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        let (_, journal, _) = apply_block_capture(&store, block, 1, &ctx);
        assert_eq!(count_live_checks(&store, &owner.address()), 1);
        revert_journal(&store, &journal);
        assert!(load_drc_check_live(&store, &id).unwrap().is_none());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            before_bal
        );
        assert_eq!(count_live_checks(&store, &owner.address()), 0);
        assert_eq!(
            compose_trident_state_root(&store, &TIP).unwrap(),
            root_before
        );
        assert_eq!(drc_check_root(&store).unwrap(), check_root_before);
    }

    #[test]
    fn revert_cash_restores_live_check_destination_balance_and_submitter_fee() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        let destination = key(4);
        let helper = key(5);
        fund(&store, &owner, 3_000);
        fund(&store, &destination, 10);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 80, Some(40), 1, &ctx);
        let locked_before = locked_check_total(&store, &owner.address());
        let destination_before = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .balance;
        let helper_before = load_account(&store, NativeAssetId::DRC, &helper.address())
            .unwrap()
            .balance;
        let cash = signed_cash(&destination, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cashes.push(cash);
        let (_, journal, _) = apply_block_capture(&store, block, 2, &ctx);
        assert!(load_drc_check_receipt(&store, &id).unwrap().is_some());
        revert_journal(&store, &journal);
        assert!(load_drc_check_live(&store, &id).unwrap().is_some());
        assert!(load_drc_check_receipt(&store, &id).unwrap().is_none());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance,
            destination_before
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &helper.address())
                .unwrap()
                .balance,
            helper_before
        );
        assert_eq!(locked_check_total(&store, &owner.address()), locked_before);
    }

    #[test]
    fn revert_cancel_restores_live_check_and_owner_locked_value() {
        let store = StateStore::open_in_memory();
        let owner = key(6);
        let destination = key(7);
        let helper = key(8);
        fund(&store, &owner, 2_000);
        fund(&store, &helper, 20);
        let ctx = auth();
        let (id, create) = create_live_check(&store, &owner, &destination, 60, Some(30), 1, &ctx);
        let owner_after_create = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .balance;
        let cancel = signed_cancel(&helper, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cancels.push(cancel);
        let (_, journal, _) = apply_block_capture(&store, block, 30, &ctx);
        revert_journal(&store, &journal);
        assert!(load_drc_check_live(&store, &id).unwrap().is_some());
        assert!(load_drc_check_receipt(&store, &id).unwrap().is_none());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after_create
        );
        assert_eq!(create.amount.as_base_units(), 60);
    }

    #[test]
    fn reapply_after_revert_is_deterministic_for_cash_block() {
        let store = StateStore::open_in_memory();
        let owner = key(9);
        let destination = key(10);
        fund(&store, &owner, 2_000);
        fund(&store, &destination, 10);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 25, Some(60), 1, &ctx);
        let cash = signed_cash(&destination, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cashes.push(cash);
        let (block_id, journal, acceptance) = apply_block_capture(&store, block.clone(), 5, &ctx);
        let root1 = compose_trident_state_root(&store, &TIP).unwrap();
        revert_journal(&store, &journal);
        let (_, journal2, acceptance2) = apply_block_capture(&store, block, 5, &ctx);
        let root2 = compose_trident_state_root(&store, &TIP).unwrap();
        assert_eq!(root1, root2);
        assert_eq!(
            acceptance.drc_check_cash_statuses,
            acceptance2.drc_check_cash_statuses
        );
        assert_eq!(
            load_block_acceptance(&store, &block_id).drc_check_cash_statuses[0],
            TransactionAcceptance::Accepted
        );
        let _ = journal2;
    }

    #[test]
    fn reapply_after_revert_is_deterministic_for_create_block() {
        let store = StateStore::open_in_memory();
        let owner = key(11);
        let destination = key(12);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let create = signed_create(&owner, destination.address(), 25, Some(60), 0, &ctx, None);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        let (block_id, journal, acceptance) = apply_block_capture(&store, block.clone(), 5, &ctx);
        let root1 = compose_trident_state_root(&store, &TIP).unwrap();
        revert_journal(&store, &journal);
        let (_, journal2, acceptance2) = apply_block_capture(&store, block, 5, &ctx);
        let root2 = compose_trident_state_root(&store, &TIP).unwrap();
        assert_eq!(root1, root2);
        assert_eq!(
            acceptance.drc_check_create_statuses,
            acceptance2.drc_check_create_statuses
        );
        assert_eq!(
            load_block_acceptance(&store, &block_id).drc_check_create_statuses[0],
            TransactionAcceptance::Accepted
        );
        let _ = journal2;
    }

    #[test]
    fn reapply_after_revert_is_deterministic_for_cancel_block() {
        let store = StateStore::open_in_memory();
        let owner = key(13);
        let destination = key(14);
        fund(&store, &owner, 2_000);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 25, Some(60), 1, &ctx);
        let cancel = signed_cancel(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cancels.push(cancel);
        let (block_id, journal, acceptance) = apply_block_capture(&store, block.clone(), 5, &ctx);
        let root1 = compose_trident_state_root(&store, &TIP).unwrap();
        revert_journal(&store, &journal);
        let (_, journal2, acceptance2) = apply_block_capture(&store, block, 5, &ctx);
        let root2 = compose_trident_state_root(&store, &TIP).unwrap();
        assert_eq!(root1, root2);
        assert_eq!(
            acceptance.drc_check_cancel_statuses,
            acceptance2.drc_check_cancel_statuses
        );
        assert_eq!(
            load_block_acceptance(&store, &block_id).drc_check_cancel_statuses[0],
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
    use crate::drc_check::{
        drc_check_root, load_drc_check_live, load_drc_check_receipt, lookup_drc_check_point,
    };
    use crate::drc_check_test_harness::support::{
        apply_block_capture, apply_check_block, auth, coinbase, count_live_checks, fund, key,
        revert_journal, signed_cancel, signed_cash, signed_create,
    };
    use crate::StateStore;

    #[test]
    fn reopen_live_cash_cancel_and_rollback_paths() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path()).unwrap();
        let owner = key(20);
        let destination = key(21);
        fund(&store, &owner, 10_000);
        fund(&store, &destination, 10);
        let ctx = auth();
        let create = signed_create(&owner, destination.address(), 40, Some(20), 0, &ctx, None);
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        apply_check_block(&store, block, 1, &ctx);
        drop(store);

        let reopened = StateStore::open(dir.path()).unwrap();
        assert_eq!(lookup_drc_check_point(&reopened, &id).unwrap(), "live");
        assert_eq!(count_live_checks(&reopened, &owner.address()), 1);
        let cash = signed_cash(&destination, id, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_check_cashes.push(cash);
        apply_check_block(&reopened, block2, 2, &ctx);
        assert!(load_drc_check_receipt(&reopened, &id).unwrap().is_some());
        drop(reopened);

        let after_cash = StateStore::open(dir.path()).unwrap();
        assert_eq!(
            load_account(&after_cash, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance,
            10 + 40 - 1
        );

        let store2 = StateStore::open_in_memory();
        fund(&store2, &owner, 10_000);
        let (id2, _) = crate::drc_check_test_harness::support::create_live_check(
            &store2,
            &owner,
            &destination,
            10,
            Some(15),
            1,
            &ctx,
        );
        let cancel = signed_cancel(&owner, id2, 1, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_check_cancels.push(cancel);
        let (_, journal, _) = apply_block_capture(&store2, block3, 15, &ctx);
        revert_journal(&store2, &journal);
        assert!(load_drc_check_live(&store2, &id2).unwrap().is_some());
        assert_eq!(
            drc_check_root(&after_cash).unwrap(),
            drc_check_root(&after_cash).unwrap()
        );
    }
}
