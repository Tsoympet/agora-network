//! CORE-HARDENING consensus matrices (boundaries, preservation, repeat schedule).

#[cfg(test)]
mod boundaries {
    use agora_types::{
        payment_channel_finalize_allowed, payment_channel_owner_schedule_deadline, Amount,
        DrcPaymentChannelCloseKind, DrcPaymentChannelError, Hash,
    };

    use crate::accounts::load_account;
    use crate::drc_payment_channel::{
        load_drc_payment_channel_claim_event, load_drc_payment_channel_fund_event,
        load_drc_payment_channel_live, load_drc_payment_channel_schedule_event,
    };
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, create_live_channel_at_score, fund, key,
        reject_channel_apply_preserving_state, signed_claim, signed_close, signed_fund,
        snapshot_channel_state,
    };
    use crate::StateStore;
    use agora_types::NativeAssetId;

    #[test]
    fn finalize_inclusive_at_scheduled_and_cancel_after_boundaries() {
        assert!(!payment_channel_finalize_allowed(9, Some(10), None));
        assert!(payment_channel_finalize_allowed(10, Some(10), None));
        assert!(!payment_channel_finalize_allowed(49, None, Some(50)));
        assert!(payment_channel_finalize_allowed(50, None, Some(50)));
    }

    #[test]
    fn owner_schedule_resets_deadline_on_repeat() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(1);
        let dest = key(2);
        let claim_key = key(3);
        fund(&store, &owner, 500);
        fund(&store, &dest, 5);
        let (channel_id, _) =
            create_live_channel_at_score(&store, &owner, &claim_key, &dest, 100, 0, &ctx, 10);
        let schedule1 = signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            1,
            &ctx,
        );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(schedule1.clone());
        apply_channel_block(&store, block, 20, &ctx);
        let live = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap();
        assert_eq!(live.close_finalizable_after, Some(25));
        load_drc_payment_channel_schedule_event(&store, &schedule1.close_tx_id())
            .unwrap()
            .expect("schedule event");

        let schedule2 = signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            2,
            &ctx,
        );
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_payment_channel_closes.push(schedule2);
        apply_channel_block(&store, block2, 30, &ctx);
        let live2 = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap();
        assert_eq!(live2.close_finalizable_after, Some(35));
    }

    #[test]
    fn fund_after_owner_schedule_and_writes_fund_event() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(4);
        let dest = key(5);
        let claim_key = key(6);
        fund(&store, &owner, 800);
        let (channel_id, _) =
            create_live_channel_at_score(&store, &owner, &claim_key, &dest, 50, 0, &ctx, 1);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            1,
            &ctx,
        ));
        apply_channel_block(&store, block, 2, &ctx);
        let fund_tx = signed_fund(&owner, channel_id, 10, 2, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_payment_channel_funds.push(fund_tx.clone());
        apply_channel_block(&store, block2, 3, &ctx);
        let live = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap();
        assert_eq!(live.total_funded, Amount::from_base_units(60));
        let ev = load_drc_payment_channel_fund_event(&store, &fund_tx.fund_tx_id())
            .unwrap()
            .unwrap();
        assert_eq!(ev.application_blue_score, 3);
    }

    #[test]
    fn claim_during_settle_delay_and_failed_fund_preserves_state() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(7);
        let dest = key(8);
        let claim_key = key(9);
        fund(&store, &owner, 500);
        fund(&store, &dest, 5);
        let (channel_id, _) =
            create_live_channel_at_score(&store, &owner, &claim_key, &dest, 100, 0, &ctx, 1);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            1,
            &ctx,
        ));
        apply_channel_block(&store, block, 2, &ctx);
        let claim = signed_claim(&dest, &claim_key, channel_id, 25, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &dest);
        block2.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block2, 3, &ctx);

        let before = snapshot_channel_state(&store, &owner, &dest);
        let bad_fund = signed_fund(&dest, channel_id, 5, 0, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &dest);
        block3.drc_payment_channel_funds.push(bad_fund);
        reject_channel_apply_preserving_state(&store, &owner, &dest, &block3, &ctx, &before, 4);
    }

    #[test]
    fn create_rejects_cancel_after_not_future_and_settle_delay_overflow() {
        assert!(matches!(
            agora_types::payment_channel_cancel_after_valid_at_create(10, Some(10)),
            Err(DrcPaymentChannelError::CancelAfterNotFuture)
        ));
        assert!(payment_channel_owner_schedule_deadline(u64::MAX - 1, 2).is_err());
    }

    #[test]
    fn finalize_rejected_one_before_scheduled_deadline() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(11);
        let dest = key(12);
        let claim_key = key(13);
        fund(&store, &owner, 400);
        fund(&store, &dest, 5);
        let (channel_id, _) =
            create_live_channel_at_score(&store, &owner, &claim_key, &dest, 80, 0, &ctx, 5);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            1,
            &ctx,
        ));
        apply_channel_block(&store, block, 10, &ctx);
        let before = snapshot_channel_state(&store, &owner, &dest);
        let fin = signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::Finalize,
            2,
            &ctx,
        );
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_payment_channel_closes.push(fin);
        reject_channel_apply_preserving_state(&store, &owner, &dest, &block2, &ctx, &before, 14);
        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::Finalize,
            2,
            &ctx,
        ));
        apply_channel_block(&store, block3, 15, &ctx);
    }

    #[test]
    fn claim_event_records_delta_and_cumulative() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(14);
        let dest = key(15);
        let claim_key = key(16);
        fund(&store, &owner, 300);
        fund(&store, &dest, 5);
        let (channel_id, _) =
            create_live_channel_at_score(&store, &owner, &claim_key, &dest, 100, 0, &ctx, 1);
        let claim = signed_claim(&dest, &claim_key, channel_id, 40, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim.clone());
        apply_channel_block(&store, block, 2, &ctx);
        let ev = load_drc_payment_channel_claim_event(&store, &claim.claim_tx_id())
            .unwrap()
            .unwrap();
        assert_eq!(ev.claim_delta, Amount::from_base_units(40));
        assert_eq!(ev.cumulative_authorized, Amount::from_base_units(40));
    }
}

#[cfg(test)]
mod claim_close_matrix {
    use agora_crypto::sign_payment_channel_offledger_claim;
    use agora_types::{
        payment_channel_onchain_claim_allowed, Amount, DrcPaymentChannelCloseKind, Hash,
    };

    use crate::accounts::load_account;
    use crate::drc_payment_channel::load_drc_payment_channel_receipt;
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, fund, key, reject_channel_apply_preserving_state,
        signed_claim, signed_close, signed_create, snapshot_channel_invariants,
        snapshot_channel_state,
    };
    use crate::StateStore;
    use agora_types::NativeAssetId;

    fn channel_with_cancel(
        store: &StateStore,
        cancel: u64,
    ) -> (
        agora_crypto::KeyPair,
        agora_crypto::KeyPair,
        agora_crypto::KeyPair,
        Hash,
    ) {
        let ctx = auth();
        let owner = key(30);
        let dest = key(31);
        let claim_key = key(32);
        fund(store, &owner, 500);
        fund(store, &dest, 20);
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let create = signed_create(
            &owner,
            &claim_key,
            dest.address(),
            100,
            nonce,
            &ctx,
            Some(0),
            None,
            Some(cancel),
            5,
        );
        let channel_id = create.channel_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_creates.push(create);
        apply_channel_block(store, block, 10, &ctx);
        (owner, dest, claim_key, channel_id)
    }

    #[test]
    fn claim_equal_lower_excess_and_replay_rejected() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(40);
        let dest = key(41);
        let claim_key = key(42);
        fund(&store, &owner, 500);
        fund(&store, &dest, 20);
        let (channel_id, _) =
            crate::drc_payment_channel_test_harness::support::create_live_channel_at_score(
                &store, &owner, &claim_key, &dest, 100, 0, &ctx, 1,
            );
        let n = load_account(&store, NativeAssetId::DRC, &dest.address())
            .unwrap()
            .nonce;
        let claim40 = signed_claim(&dest, &claim_key, channel_id, 40, n, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim40);
        apply_channel_block(&store, block, 2, &ctx);
        let before = snapshot_channel_state(&store, &owner, &dest);
        for cumulative in [40u64, 30, 150] {
            let mut block_fail = coinbase(vec![Hash::ZERO], &dest);
            block_fail.drc_payment_channel_claims.push(signed_claim(
                &dest,
                &claim_key,
                channel_id,
                cumulative,
                load_account(&store, NativeAssetId::DRC, &dest.address())
                    .unwrap()
                    .nonce,
                &ctx,
            ));
            reject_channel_apply_preserving_state(
                &store,
                &owner,
                &dest,
                &block_fail,
                &ctx,
                &before,
                3,
            );
        }
        let replay = signed_claim(&dest, &claim_key, channel_id, 40, n, &ctx);
        let mut block_replay = coinbase(vec![Hash::ZERO], &dest);
        block_replay.drc_payment_channel_claims.push(replay);
        reject_channel_apply_preserving_state(
            &store,
            &owner,
            &dest,
            &block_replay,
            &ctx,
            &before,
            4,
        );
    }

    #[test]
    fn on_chain_claim_rejected_at_cancel_after_allowed_one_before() {
        assert!(!payment_channel_onchain_claim_allowed(100, Some(100)));
        assert!(payment_channel_onchain_claim_allowed(99, Some(100)));
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, claim_key, channel_id) = channel_with_cancel(&store, 100);
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let n = load_account(&store, NativeAssetId::DRC, &dest.address())
            .unwrap()
            .nonce;
        let mut block_at = coinbase(vec![Hash::ZERO], &dest);
        block_at
            .drc_payment_channel_claims
            .push(signed_claim(&dest, &claim_key, channel_id, 25, n, &ctx));
        reject_channel_apply_preserving_state(
            &store,
            &owner,
            &dest,
            &block_at,
            &ctx,
            &before.base,
            100,
        );
        let mut block_before = coinbase(vec![Hash::ZERO], &dest);
        block_before
            .drc_payment_channel_claims
            .push(signed_claim(&dest, &claim_key, channel_id, 25, n, &ctx));
        apply_channel_block(&store, block_before, 99, &ctx);
    }

    #[test]
    fn incremental_claims_during_settle_delay_and_destination_immediate_close() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(50);
        let dest = key(51);
        let claim_key = key(52);
        fund(&store, &owner, 500);
        fund(&store, &dest, 20);
        let (channel_id, _) =
            crate::drc_payment_channel_test_harness::support::create_live_channel_at_score(
                &store, &owner, &claim_key, &dest, 100, 0, &ctx, 1,
            );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            1,
            &ctx,
        ));
        apply_channel_block(&store, block, 2, &ctx);
        let n = load_account(&store, NativeAssetId::DRC, &dest.address())
            .unwrap()
            .nonce;
        let mut block2 = coinbase(vec![Hash::ZERO], &dest);
        block2
            .drc_payment_channel_claims
            .push(signed_claim(&dest, &claim_key, channel_id, 20, n, &ctx));
        apply_channel_block(&store, block2, 3, &ctx);
        let mut block3 = coinbase(vec![Hash::ZERO], &dest);
        block3.drc_payment_channel_claims.push(signed_claim(
            &dest,
            &claim_key,
            channel_id,
            35,
            load_account(&store, NativeAssetId::DRC, &dest.address())
                .unwrap()
                .nonce,
            &ctx,
        ));
        apply_channel_block(&store, block3, 4, &ctx);

        let (owner2, dest2, claim_key2, channel_id2) = channel_with_cancel(&store, 200);
        fund(&store, &dest2, 5);
        let dest_bal_before = load_account(&store, NativeAssetId::DRC, &dest2.address())
            .unwrap()
            .balance;
        let owner_bal_before = load_account(&store, NativeAssetId::DRC, &owner2.address())
            .unwrap()
            .balance;
        let root_before = crate::drc_payment_channel_test_harness::support::channel_root(&store);
        let mut block4 = coinbase(vec![Hash::ZERO], &dest2);
        block4.drc_payment_channel_closes.push(signed_close(
            &dest2,
            channel_id2,
            DrcPaymentChannelCloseKind::DestinationClose,
            load_account(&store, NativeAssetId::DRC, &dest2.address())
                .unwrap()
                .nonce,
            &ctx,
        ));
        apply_channel_block(&store, block4, 20, &ctx);
        assert!(load_drc_payment_channel_receipt(&store, &channel_id2)
            .unwrap()
            .is_some());
        assert!(
            load_account(&store, NativeAssetId::DRC, &owner2.address())
                .unwrap()
                .balance
                > owner_bal_before
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &dest2.address())
                .unwrap()
                .balance,
            dest_bal_before - 1
        );
        assert_ne!(
            crate::drc_payment_channel_test_harness::support::channel_root(&store),
            root_before
        );
        let _ = claim_key2;
    }

    #[test]
    fn claim_fee_and_delta_update_destination_balance_atomically() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(60);
        let dest = key(61);
        let claim_key = key(62);
        fund(&store, &owner, 500);
        fund(&store, &dest, 50);
        let (channel_id, _) =
            crate::drc_payment_channel_test_harness::support::create_live_channel_at_score(
                &store, &owner, &claim_key, &dest, 100, 0, &ctx, 1,
            );
        let dest_before = load_account(&store, NativeAssetId::DRC, &dest.address()).unwrap();
        let owner_before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let cumulative = 45u64;
        let delta = cumulative;
        let fee = 1u64;
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(signed_claim(
            &dest,
            &claim_key,
            channel_id,
            cumulative,
            dest_before.nonce,
            &ctx,
        ));
        apply_channel_block(&store, block, 2, &ctx);
        let dest_after = load_account(&store, NativeAssetId::DRC, &dest.address()).unwrap();
        let owner_after = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(dest_after.balance, dest_before.balance + delta - fee);
        assert_eq!(
            owner_after.balance,
            owner_before.balance,
            "claim debits locked remainder only, not owner liquid balance"
        );
    }

    #[test]
    fn bad_offledger_with_valid_on_chain_signature_rejected() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, claim_key, channel_id) = channel_with_cancel(&store, 300);
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let bad = sign_payment_channel_offledger_claim(
            &claim_key,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            Amount::from_base_units(999),
        )
        .unwrap();
        let mut claim = signed_claim(
            &dest,
            &claim_key,
            channel_id,
            50,
            load_account(&store, NativeAssetId::DRC, &dest.address())
                .unwrap()
                .nonce,
            &ctx,
        );
        claim.channel_claim_signature = bad.to_vec();
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim);
        reject_channel_apply_preserving_state(
            &store,
            &owner,
            &dest,
            &block,
            &ctx,
            &before.base,
            50,
        );
    }
}
