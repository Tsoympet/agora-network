//! CHECKS-AUTH-BEHAVIOR tranche: tags, blue-score cutoffs, deposit auth, cancel rules,
//! conservation, tickets, master-key, and deterministic invariant sequences.

#[cfg(test)]
mod dest_tag_and_invoice {
    use agora_types::{DrcCheckCreateTx, DrcCheckError, Hash, DRC_CHECK_CREATE_TX_VERSION};
    use borsh::BorshDeserialize;

    use crate::accounts::load_account;
    use crate::drc_check::{load_drc_check_live, load_drc_check_receipt};
    use crate::drc_check_test_harness::support::{
        apply_check_block, auth, coinbase, fund, key, signed_cash, signed_create,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    #[test]
    fn some_zero_tag_satisfies_require_dest_tag_and_round_trips() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let destination = key(2);
        fund(&store, &owner, 5_000);
        fund(&store, &destination, 100);
        let ctx = auth();
        let mut pol = agora_types::DrcAccountPolicyTx::set_require_destination_tag(
            destination.address(),
            agora_types::Amount::ZERO,
            0,
        );
        agora_crypto::sign_drc_account_policy_bound(
            &mut pol,
            &destination,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();

        let create = signed_create(&owner, destination.address(), 25, None, 0, &ctx, Some(0));
        assert_eq!(create.authenticated_destination_tag(), Some(0));
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create.clone());
        apply_check_block(&store, block, 1, &ctx);
        let live = load_drc_check_live(&store, &id).unwrap().unwrap();
        assert_eq!(live.destination_tag, Some(0));
        assert_eq!(live.source_tag, None);

        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_check_cashes.push(cash);
        apply_check_block(&store, block2, 1, &ctx);
        let receipt = load_drc_check_receipt(&store, &id).unwrap().unwrap();
        assert_eq!(receipt.destination_tag, Some(0));

        let bytes = borsh::to_vec(&create).unwrap();
        let decoded = DrcCheckCreateTx::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.destination_tag, Some(0));
    }

    #[test]
    fn nonzero_invoice_id_rejected_without_state_mutation() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        fund(&store, &owner, 1_000);
        let before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let tx = DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TX_VERSION,
            owner: owner.address(),
            destination: key(4).address(),
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash([9; 32]),
            expires_after_blue_score: Some(100),
            nonce: before.nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(matches!(
            tx.validate_structure(),
            Err(DrcCheckError::NonZeroInvoiceNotSupported)
        ));
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            before
        );
    }
}

#[cfg(test)]
mod cutoff_deterministic {
    use agora_types::{
        validate_check_expiration_bound, Hash, TransactionAcceptance,
        DRC_CHECK_MAX_BLUE_SCORE_BOUND,
    };

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_virtual_at_blue_score;
    use crate::drc_check::{apply_drc_check_cancel, apply_drc_check_cash};
    use crate::drc_check_test_harness::support::{
        apply_check_block, auth, coinbase, create_live_check, fund, key, signed_cancel,
        signed_cash, signed_create,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    fn apply_cash_at(
        store: &StateStore,
        id: Hash,
        score: u64,
        destination: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) -> bool {
        let nonce = load_account(store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(destination, id, nonce, ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_check_cash(store, &cash, ctx, score, &mut batch, &mut journal).is_ok()
    }

    #[allow(dead_code)]
    fn apply_cancel_at(
        store: &StateStore,
        id: Hash,
        score: u64,
        submitter: &agora_crypto::KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) -> bool {
        let nonce = load_account(store, NativeAssetId::DRC, &submitter.address())
            .unwrap()
            .nonce;
        let cancel = signed_cancel(submitter, id, nonce, ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_check_cancel(store, &cancel, ctx, score, &mut batch, &mut journal).is_ok()
    }

    #[test]
    fn create_bound_validation_matrix() {
        assert!(validate_check_expiration_bound(None).is_ok());
        assert!(validate_check_expiration_bound(Some(0)).is_ok());
        assert!(validate_check_expiration_bound(Some(DRC_CHECK_MAX_BLUE_SCORE_BOUND)).is_ok());
        assert!(validate_check_expiration_bound(Some(DRC_CHECK_MAX_BLUE_SCORE_BOUND + 1)).is_err());
    }

    #[test]
    fn no_expiration_cash_at_any_score() {
        let store = StateStore::open_in_memory();
        let owner = key(10);
        let destination = key(11);
        fund(&store, &owner, 2_000);
        fund(&store, &destination, 5);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 10, None, 1, &ctx);
        assert!(apply_cash_at(&store, id, 0, &destination, &ctx));
        assert!(apply_cash_at(&store, id, u64::MAX / 2, &destination, &ctx));
    }

    #[test]
    fn cash_one_before_exact_after_expiration_boundary() {
        let store = StateStore::open_in_memory();
        let owner = key(12);
        let destination = key(13);
        fund(&store, &owner, 2_000);
        fund(&store, &destination, 5);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 10, Some(30), 1, &ctx);
        assert!(apply_cash_at(&store, id, 29, &destination, &ctx));
        let (id2, _) = create_live_check(&store, &owner, &destination, 10, Some(30), 1, &ctx);
        assert!(!apply_cash_at(&store, id2, 30, &destination, &ctx));
        let (id3, _) = create_live_check(&store, &owner, &destination, 10, Some(30), 1, &ctx);
        assert!(!apply_cash_at(&store, id3, 31, &destination, &ctx));
    }

    #[test]
    fn same_block_create_then_cash_uses_application_score() {
        let store = StateStore::open_in_memory();
        let owner = key(14);
        let destination = key(15);
        fund(&store, &owner, 500);
        fund(&store, &destination, 5);
        let ctx = auth();
        let create = signed_create(&owner, destination.address(), 20, Some(10), 0, &ctx, None);
        let id = create.check_id();
        let cash = signed_cash(&destination, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        block.drc_check_cashes.push(cash);
        apply_check_block(&store, block, 9, &ctx);
        assert!(crate::drc_check::load_drc_check_receipt(&store, &id)
            .unwrap()
            .is_some());
    }

    #[test]
    fn virtual_lane_cash_vs_cancel_first_wins_settlement() {
        let store = StateStore::open_in_memory();
        let owner = key(16);
        let destination = key(17);
        fund(&store, &owner, 500);
        fund(&store, &destination, 10);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 25, None, 1, &ctx);
        let cash = signed_cash(&destination, id, 0, &ctx);
        let cancel = signed_cancel(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cashes.push(cash);
        block.drc_check_cancels.push(cancel);
        block.header.tx_root = block.compute_body_root();
        let v =
            apply_block_batched_virtual_at_blue_score(&store, &block, 50, Some(&ctx), 50).unwrap();
        assert_eq!(
            v.acceptance.drc_check_cash_statuses[0],
            TransactionAcceptance::Accepted
        );
        assert_eq!(
            v.acceptance.drc_check_cancel_statuses[0],
            TransactionAcceptance::ExactDuplicate
        );
    }

    #[test]
    fn create_at_max_bound_then_cash_before_expiry() {
        let store = StateStore::open_in_memory();
        let owner = key(18);
        let destination = key(19);
        fund(&store, &owner, 2_000);
        fund(&store, &destination, 5);
        let ctx = auth();
        let create = signed_create(
            &owner,
            destination.address(),
            5,
            Some(DRC_CHECK_MAX_BLUE_SCORE_BOUND),
            0,
            &ctx,
            None,
        );
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        apply_check_block(&store, block, 1, &ctx);
        assert!(apply_cash_at(
            &store,
            id,
            DRC_CHECK_MAX_BLUE_SCORE_BOUND - 1,
            &destination,
            &ctx
        ));
    }
}

#[cfg(test)]
mod cancel_authorization {
    use agora_types::Hash;

    use crate::drc_check_test_harness::support::{
        apply_check_block, auth, coinbase, create_live_check, fund, key, signed_cancel,
    };
    use crate::StateStore;

    #[test]
    fn owner_and_destination_cancel_before_expiry_stranger_rejected() {
        let store = StateStore::open_in_memory();
        let owner = key(20);
        let destination = key(21);
        let stranger = key(22);
        fund(&store, &owner, 500);
        fund(&store, &destination, 5);
        fund(&store, &stranger, 50);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 10, Some(50), 1, &ctx);
        let cancel = signed_cancel(&owner, id, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cancels.push(cancel);
        apply_check_block(&store, block, 10, &ctx);

        let (id2, _) = create_live_check(&store, &owner, &destination, 10, Some(50), 2, &ctx);
        let cancel2 = signed_cancel(&destination, id2, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &destination);
        block2.drc_check_cancels.push(cancel2);
        apply_check_block(&store, block2, 10, &ctx);

        let (id3, _) = create_live_check(&store, &owner, &destination, 10, Some(50), 3, &ctx);
        let cancel3 = signed_cancel(&stranger, id3, 0, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &stranger);
        block3.drc_check_cancels.push(cancel3);
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block3,
            50,
            Some(&ctx),
            10
        )
        .is_err());
        assert_eq!(
            crate::drc_check::lookup_drc_check_point(&store, &id3).unwrap(),
            "live"
        );
    }

    #[test]
    fn exact_expiration_rejects_cash_allows_stranger_cancel() {
        let store = StateStore::open_in_memory();
        let owner = key(23);
        let destination = key(24);
        let stranger = key(25);
        fund(&store, &owner, 500);
        fund(&store, &destination, 5);
        fund(&store, &stranger, 50);
        let ctx = auth();
        let (id, _) = create_live_check(&store, &owner, &destination, 10, Some(40), 1, &ctx);
        let cash = crate::drc_check_test_harness::support::signed_cash(&destination, id, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &destination);
        block.drc_check_cashes.push(cash);
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            40
        )
        .is_err());
        let cancel = signed_cancel(&stranger, id, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &stranger);
        block2.drc_check_cancels.push(cancel);
        apply_check_block(&store, block2, 40, &ctx);
        assert_eq!(
            crate::drc_check::lookup_drc_check_point(&store, &id).unwrap(),
            "unknown"
        );
    }
}

#[cfg(test)]
mod policy_composition {
    use agora_crypto::{sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound};
    use agora_types::{DrcAccountPolicyTx, DrcDepositPreauthTx, Hash, NativeAssetId};

    use crate::accounts::load_account;
    use crate::drc_check::{apply_drc_check_cash, load_drc_check_live};
    use crate::drc_check_test_harness::support::{
        apply_check_block, assert_check_snapshot_unchanged, auth, coinbase, create_live_check,
        fund, key, signed_cash, signed_create, snapshot_check_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn deposit_auth_destination_cash_succeeds_without_preauth() {
        let store = StateStore::open_in_memory();
        let owner = key(30);
        let destination = key(31);
        fund(&store, &owner, 3_000);
        fund(&store, &destination, 10);
        let ctx = auth();
        let mut pol = DrcAccountPolicyTx::set_deposit_auth_required(
            destination.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &destination, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let (id, _) = create_live_check(&store, &owner, &destination, 50, None, 1, &ctx);
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cashes.push(cash);
        apply_check_block(&store, block, 2, &ctx);
        assert!(load_drc_check_live(&store, &id).unwrap().is_none());
    }

    #[test]
    fn deposit_auth_revoke_after_create_does_not_block_destination_cash() {
        let store = StateStore::open_in_memory();
        let owner = key(32);
        let destination = key(33);
        fund(&store, &owner, 3_000);
        fund(&store, &destination, 10);
        let ctx = auth();
        let mut pol = DrcAccountPolicyTx::set_deposit_auth_required(
            destination.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &destination, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let mut pre = DrcDepositPreauthTx::authorize(
            destination.address(),
            owner.address(),
            agora_types::Amount::ZERO,
            dest_nonce,
        );
        sign_drc_deposit_preauth_bound(&mut pre, &destination, &ctx.chain_id, &ctx.genesis)
            .unwrap();
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
        let (id, _) = create_live_check(&store, &owner, &destination, 40, None, 1, &ctx);
        let dest_nonce2 = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let mut revoke = DrcDepositPreauthTx::unauthorize(
            destination.address(),
            owner.address(),
            agora_types::Amount::ZERO,
            dest_nonce2,
        );
        sign_drc_deposit_preauth_bound(&mut revoke, &destination, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_deposit_preauth::apply_drc_deposit_preauth(
            &store,
            &revoke,
            &ctx,
            &mut batch,
            &mut journal,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
        let dest_nonce3 = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce3, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_check_cash(&store, &cash, &ctx, 2, &mut batch, &mut journal).is_ok());
    }

    #[test]
    fn insufficient_owner_balance_leaves_check_live() {
        let store = StateStore::open_in_memory();
        let owner = key(34);
        let destination = key(35);
        fund(&store, &owner, 5);
        fund(&store, &destination, 10);
        let ctx = auth();
        let mut pol = DrcAccountPolicyTx::set_deposit_auth_required(
            destination.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &destination, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let (id, _) = create_live_check(&store, &owner, &destination, 40, None, 1, &ctx);
        let snap = snapshot_check_state(&store, &owner, &destination);
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_check_cash(&store, &cash, &ctx, 2, &mut batch, &mut journal).is_err());
        assert_check_snapshot_unchanged(&store, &owner, &destination, &snap);
        assert_eq!(
            crate::drc_check::lookup_drc_check_point(&store, &id).unwrap(),
            "live"
        );
    }

    #[test]
    fn require_dest_tag_enabled_after_untagged_create_grandfathered_cash() {
        let store = StateStore::open_in_memory();
        let owner = key(36);
        let destination = key(37);
        fund(&store, &owner, 2_000);
        fund(&store, &destination, 5);
        let ctx = auth();
        let create = signed_create(&owner, destination.address(), 10, None, 0, &ctx, None);
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        apply_check_block(&store, block, 1, &ctx);
        let mut pol = DrcAccountPolicyTx::set_require_destination_tag(
            destination.address(),
            agora_types::Amount::ZERO,
            0,
        );
        sign_drc_account_policy_bound(&mut pol, &destination, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_check_cashes.push(cash);
        apply_check_block(&store, block2, 2, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance,
            5 + 10 - 1
        );
    }
}

#[cfg(test)]
mod value_conservation {
    use agora_types::{Hash, NativeAssetId, DRC_MAX_LIVE_CHECKS_PER_ACCOUNT};

    use crate::accounts::load_account;
    use crate::drc_check_test_harness::support::{
        apply_check_block, auth, coinbase, count_live_checks, create_live_check, fund, key,
        signed_cancel, signed_cash, signed_create, spendable_plus_locked,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn cap_thirty_two_then_cancel_admits_thirty_third() {
        let store = StateStore::open_in_memory();
        let owner = key(40);
        let destination = key(41);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let mut nonce = 0u64;
        let mut ids = Vec::new();
        for i in 0..DRC_MAX_LIVE_CHECKS_PER_ACCOUNT {
            let create = signed_create(
                &owner,
                destination.address(),
                1,
                Some(200 + i as u64),
                nonce,
                &ctx,
                None,
            );
            ids.push(create.check_id());
            let mut block = coinbase(vec![Hash::ZERO], &owner);
            block.drc_check_creates.push(create);
            apply_check_block(&store, block, 1, &ctx);
            nonce += 1;
        }
        assert_eq!(count_live_checks(&store, &owner.address()), 32);
        let fail = signed_create(
            &owner,
            destination.address(),
            1,
            Some(9999),
            nonce,
            &ctx,
            None,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_check::apply_drc_check_create(
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
        block.drc_check_cancels.push(cancel);
        apply_check_block(&store, block, 200, &ctx);
        let create2 = signed_create(
            &owner,
            destination.address(),
            1,
            Some(5000),
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            &ctx,
            None,
        );
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_check_creates.push(create2);
        apply_check_block(&store, block2, 1, &ctx);
        assert_eq!(count_live_checks(&store, &owner.address()), 32);
    }

    #[test]
    fn cash_debits_owner_and_credits_destination_once_destination_pays_fee() {
        let store = StateStore::open_in_memory();
        let owner = key(42);
        let destination = key(43);
        fund(&store, &owner, 500);
        fund(&store, &destination, 10);
        let ctx = auth();
        let owner_before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let dest_before = load_account(&store, NativeAssetId::DRC, &destination.address()).unwrap();
        let create = signed_create(&owner, destination.address(), 100, None, 0, &ctx, None);
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        apply_check_block(&store, block, 1, &ctx);
        let owner_after_create =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(owner_after_create.balance, owner_before.balance - 1);
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_check_cashes.push(cash);
        apply_check_block(&store, block2, 1, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after_create.balance - 100
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance,
            dest_before.balance + 100 - 1
        );
    }

    #[test]
    fn cancel_moves_no_amount_submitter_pays_fee_only() {
        let store = StateStore::open_in_memory();
        let owner = key(44);
        let destination = key(45);
        let helper = key(46);
        fund(&store, &owner, 500);
        fund(&store, &helper, 20);
        let ctx = auth();
        let owner_before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let create = signed_create(&owner, destination.address(), 100, Some(50), 0, &ctx, None);
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        apply_check_block(&store, block, 1, &ctx);
        let owner_after_create =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(owner_after_create.balance, owner_before.balance - 1);
        let cancel = signed_cancel(&helper, id, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_check_cancels.push(cancel);
        apply_check_block(&store, block2, 50, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after_create.balance
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &helper.address())
                .unwrap()
                .balance,
            19
        );
    }

    #[test]
    fn aggregate_spendable_value_conserved_except_fees() {
        let store = StateStore::open_in_memory();
        let owner = key(47);
        let destination = key(48);
        fund(&store, &owner, 1_000);
        fund(&store, &destination, 50);
        let ctx = auth();
        let baseline = spendable_plus_locked(&store, &owner, &destination);
        let (id, _) = create_live_check(&store, &owner, &destination, 200, None, 1, &ctx);
        assert_eq!(
            spendable_plus_locked(&store, &owner, &destination),
            baseline + 200 - 1
        );
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let cash = signed_cash(&destination, id, dest_nonce, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_cashes.push(cash);
        apply_check_block(&store, block, 2, &ctx);
        assert_eq!(
            spendable_plus_locked(&store, &owner, &destination),
            baseline - 2
        );
    }
}

#[cfg(test)]
mod ticket_matrix {
    use agora_crypto::{
        sign_drc_check_cash_bound, sign_drc_check_create_bound, sign_drc_ticket_create_bound,
    };
    use agora_types::{
        DrcAccountSequenceSelector, DrcTicketCreateTx, Hash, DRC_CHECK_CASH_TICKET_VERSION,
        DRC_CHECK_CREATE_TICKET_VERSION,
    };

    use crate::apply::apply_block_batched_with_auth_at_blue_score;
    use crate::drc_check_test_harness::support::{
        apply_check_block, assert_check_snapshot_unchanged, auth, coinbase, fund, key,
        snapshot_check_state,
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
    fn ticket_check_create_cash_cancel_one_use_each() {
        let store = StateStore::open_in_memory();
        let owner = key(80);
        let destination = key(81);
        fund(&store, &owner, 500_000);
        fund(&store, &destination, 50);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx);
        let mut create = agora_types::DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TICKET_VERSION,
            owner: owner.address(),
            destination: destination.address(),
            amount: agora_types::Amount::from_base_units(10),
            fee: agora_types::Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_check_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_check_creates.push(create);
        apply_check_block(&store, block, 1, &ctx);
        assert!(!load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq));
        let seq2 = mint_ticket(&store, &destination, &ctx);
        let mut cash = agora_types::DrcCheckCashTx {
            version: DRC_CHECK_CASH_TICKET_VERSION,
            submitter: destination.address(),
            check_id: id,
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq2)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_check_cash_bound(&mut cash, &destination, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block2 = coinbase(vec![Hash::ZERO], &destination);
        block2.drc_check_cashes.push(cash);
        apply_check_block(&store, block2, 1, &ctx);
    }

    #[test]
    fn unknown_ticket_rejected_create_preserves_tickets() {
        let store = StateStore::open_in_memory();
        let owner = key(82);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let before = snapshot_check_state(&store, &owner, &key(83));
        let mut create = agora_types::DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TICKET_VERSION,
            owner: owner.address(),
            destination: key(83).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(50),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(999)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_check_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_check::apply_drc_check_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_check_snapshot_unchanged(&store, &owner, &key(83), &before);
        assert_eq!(
            lookup_drc_ticket_point(&store, &owner.address(), 999).unwrap(),
            crate::drc_mempool::DrcTicketPointStatus::Unknown
        );
    }

    #[test]
    fn legacy_check_version_rejects_ticket_selector() {
        let store = StateStore::open_in_memory();
        let owner = key(84);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx);
        let create = agora_types::DrcCheckCreateTx {
            version: agora_types::DRC_CHECK_CREATE_TX_VERSION,
            owner: owner.address(),
            destination: key(85).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(50),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(create.validate_structure().is_err());
    }
}

#[cfg(test)]
mod master_auth {
    use agora_crypto::{
        sign_drc_account_policy_bound, sign_drc_check_create_bound, sign_drc_regular_key_bound,
    };
    use agora_types::{DrcAccountPolicyTx, DrcRegularKeyTx};

    use crate::accounts::load_account;
    use crate::drc_check_test_harness::support::{
        assert_check_snapshot_unchanged, auth, fund, key, signed_create, snapshot_check_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    #[test]
    fn disabled_master_check_create_rejected_regular_key_succeeds() {
        let store = StateStore::open_in_memory();
        let master = key(90);
        let regular = key(91);
        let destination = key(92);
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
        let before = snapshot_check_state(&store, &master, &destination);
        let nonce = load_account(&store, agora_types::NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = signed_create(&master, destination.address(), 5, None, nonce, &ctx, None);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_check::apply_drc_check_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_check_snapshot_unchanged(&store, &master, &destination, &before);
        sign_drc_check_create_bound(&mut create, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_check::apply_drc_check_create(
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
mod deterministic_invariant_sequence {
    use agora_types::{Hash, TransactionAcceptance};

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_virtual_at_blue_score;
    use crate::drc_check_test_harness::support::{
        apply_block_capture, apply_check_block, auth, coinbase, create_live_check, fund, key,
        revert_journal, signed_cancel, signed_cash, snapshot_check_state, spendable_plus_locked,
    };
    use crate::StateStore;

    #[test]
    fn bounded_sequence_with_rollback_snapshots() {
        let store = StateStore::open_in_memory();
        let owner = key(100);
        let destination = key(101);
        let stranger = key(102);
        fund(&store, &owner, 2_000);
        fund(&store, &destination, 100);
        fund(&store, &stranger, 50);
        let ctx = auth();
        let baseline = spendable_plus_locked(&store, &owner, &destination);
        let snap0 = snapshot_check_state(&store, &owner, &destination);

        let (id, _) = create_live_check(&store, &owner, &destination, 80, Some(60), 1, &ctx);
        let snap1 = snapshot_check_state(&store, &owner, &destination);
        assert_eq!(snap1.owner_balance, snap0.owner_balance - 1);

        let mut spend = coinbase(vec![Hash::ZERO], &owner);
        spend.header.tx_root = spend.compute_body_root();
        let (_, journal_spend, _) = apply_block_capture(&store, spend, 2, &ctx);
        revert_journal(&store, &journal_spend);
        assert_eq!(
            snapshot_check_state(&store, &owner, &destination).check_root,
            snap1.check_root
        );

        let mut batch = crate::store::WriteBatch::new();
        let mut acct = crate::accounts::load_account(
            &store,
            agora_types::NativeAssetId::DRC,
            &owner.address(),
        )
        .unwrap();
        acct.balance = 10;
        crate::accounts::put_account_into(
            &mut batch,
            agora_types::NativeAssetId::DRC,
            &owner.address(),
            &acct,
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let fail_cash = signed_cash(
            &destination,
            id,
            load_account(
                &store,
                agora_types::NativeAssetId::DRC,
                &destination.address(),
            )
            .unwrap()
            .nonce,
            &ctx,
        );
        let mut block_fail = coinbase(vec![Hash::ZERO], &owner);
        block_fail.drc_check_cashes.push(fail_cash);
        block_fail.header.tx_root = block_fail.compute_body_root();
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block_fail,
            50,
            Some(&ctx),
            2
        )
        .is_err());
        assert_eq!(
            crate::drc_check::lookup_drc_check_point(&store, &id).unwrap(),
            "live"
        );

        fund(&store, &owner, 200);
        let cash = signed_cash(
            &destination,
            id,
            load_account(
                &store,
                agora_types::NativeAssetId::DRC,
                &destination.address(),
            )
            .unwrap()
            .nonce,
            &ctx,
        );
        let mut block_cash = coinbase(vec![Hash::ZERO], &owner);
        block_cash.drc_check_cashes.push(cash);
        apply_check_block(&store, block_cash, 3, &ctx);

        let (id2, _) = create_live_check(&store, &owner, &destination, 50, Some(40), 4, &ctx);
        let cancel_pre = signed_cancel(
            &owner,
            id2,
            load_account(&store, agora_types::NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            &ctx,
        );
        let mut block_cancel = coinbase(vec![Hash::ZERO], &owner);
        block_cancel.drc_check_cancels.push(cancel_pre);
        apply_check_block(&store, block_cancel, 10, &ctx);

        let (id3, _) = create_live_check(&store, &owner, &destination, 30, Some(20), 11, &ctx);
        let cancel_post = signed_cancel(&stranger, id3, 0, &ctx);
        let mut block_post = coinbase(vec![Hash::ZERO], &stranger);
        block_post.drc_check_cancels.push(cancel_post);
        apply_check_block(&store, block_post, 20, &ctx);

        let (id4, _) = create_live_check(&store, &owner, &destination, 10, None, 21, &ctx);
        fund(&store, &destination, 5);
        let n = load_account(
            &store,
            agora_types::NativeAssetId::DRC,
            &destination.address(),
        )
        .unwrap()
        .nonce;
        let cash_a = signed_cash(&destination, id4, n, &ctx);
        let cash_b = signed_cash(&destination, id4, n + 1, &ctx);
        let mut block_dup = coinbase(vec![Hash::ZERO], &owner);
        block_dup.drc_check_cashes.push(cash_a);
        block_dup.drc_check_cashes.push(cash_b);
        block_dup.header.tx_root = block_dup.compute_body_root();
        let v = apply_block_batched_virtual_at_blue_score(&store, &block_dup, 50, Some(&ctx), 22)
            .unwrap();
        assert_eq!(
            v.acceptance.drc_check_cash_statuses[0],
            TransactionAcceptance::Accepted
        );
        assert_eq!(
            v.acceptance.drc_check_cash_statuses[1],
            TransactionAcceptance::ExactDuplicate
        );

        assert!(spendable_plus_locked(&store, &owner, &destination) <= baseline);
    }
}
