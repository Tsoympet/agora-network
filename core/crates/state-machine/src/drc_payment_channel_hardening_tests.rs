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
