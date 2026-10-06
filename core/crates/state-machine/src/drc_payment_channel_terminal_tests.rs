//! Journal/reorg/reapply and RocksDB reopen for payment channels.

#[cfg(test)]
mod reorg_journal_transitions {
    use agora_types::{Hash, NativeAssetId, TransactionAcceptance};

    use crate::accounts::load_account;
    use crate::drc_payment_channel::load_drc_payment_channel_fund_event;
    use crate::drc_payment_channel_test_harness::support::{
        apply_block_capture, auth, coinbase, create_live_channel, fund, key, locked_channel_total,
        revert_journal, signed_fund,
    };
    use crate::state_root::compose_trident_state_root;
    use crate::StateStore;

    #[test]
    fn revert_fund_restores_balances_live_total_and_fund_event() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let dest = key(2);
        let claim_key = key(3);
        fund(&store, &owner, 5_000);
        let ctx = auth();
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        let after_create = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let locked_before = locked_channel_total(&store, &owner.address());
        let fund = signed_fund(&owner, channel_id, 20, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_funds.push(fund.clone());
        let (_, journal, _) = apply_block_capture(&store, block, 2, &ctx);
        assert!(
            load_drc_payment_channel_fund_event(&store, &fund.fund_tx_id())
                .unwrap()
                .is_some()
        );
        revert_journal(&store, &journal);
        assert!(
            load_drc_payment_channel_fund_event(&store, &fund.fund_tx_id())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            after_create
        );
        assert_eq!(
            locked_channel_total(&store, &owner.address()),
            locked_before
        );
    }

    #[test]
    fn reapply_fund_block_is_deterministic() {
        let store = StateStore::open_in_memory();
        let owner = key(4);
        let dest = key(5);
        let claim_key = key(6);
        fund(&store, &owner, 3_000);
        let ctx = auth();
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 50, 0, &ctx);
        let fund = signed_fund(&owner, channel_id, 10, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_funds.push(fund);
        let (block_id, journal, acceptance) = apply_block_capture(&store, block.clone(), 3, &ctx);
        let root1 = compose_trident_state_root(
            &store,
            &crate::drc_payment_channel_test_harness::support::TIP,
        )
        .unwrap();
        revert_journal(&store, &journal);
        let (_, journal2, acceptance2) = apply_block_capture(&store, block, 3, &ctx);
        let root2 = compose_trident_state_root(
            &store,
            &crate::drc_payment_channel_test_harness::support::TIP,
        )
        .unwrap();
        assert_eq!(root1, root2);
        assert_eq!(
            acceptance.drc_payment_channel_fund_statuses,
            acceptance2.drc_payment_channel_fund_statuses
        );
        assert_eq!(
            crate::load_acceptance(&store, &block_id)
                .unwrap()
                .expect("acceptance")
                .drc_payment_channel_fund_statuses[0],
            TransactionAcceptance::Accepted
        );
        let _ = journal2;
    }
}

#[cfg(feature = "rocksdb")]
#[cfg(test)]
mod rocksdb_reopen_parity {
    use agora_types::{DrcPaymentChannelCloseKind, Hash};

    use crate::drc_payment_channel::{
        load_drc_payment_channel_live, load_drc_payment_channel_receipt,
        lookup_drc_payment_channel_point,
    };
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, create_live_channel, fund, key, signed_claim,
        signed_close,
    };
    use crate::StateStore;

    #[test]
    fn reopen_live_funded_claimed_scheduled_and_closed() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path()).unwrap();
        let owner = key(10);
        let dest = key(11);
        let claim_key = key(12);
        fund(&store, &owner, 20_000);
        fund(&store, &dest, 20);
        let ctx = auth();
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        drop(store);

        let reopened = StateStore::open(dir.path()).unwrap();
        assert_eq!(
            lookup_drc_payment_channel_point(&reopened, &channel_id).unwrap(),
            "live"
        );

        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_funds.push(
            crate::drc_payment_channel_test_harness::support::signed_fund(
                &owner, channel_id, 25, 1, &ctx,
            ),
        );
        apply_channel_block(&reopened, block, 2, &ctx);
        drop(reopened);

        let funded = StateStore::open(dir.path()).unwrap();
        assert_eq!(
            load_drc_payment_channel_live(&funded, &channel_id)
                .unwrap()
                .unwrap()
                .total_funded
                .as_base_units(),
            125
        );

        let claim = signed_claim(&dest, &claim_key, channel_id, 30, 0, &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &dest);
        block2.drc_payment_channel_claims.push(claim);
        apply_channel_block(&funded, block2, 3, &ctx);

        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            2,
            &ctx,
        ));
        apply_channel_block(&funded, block3, 4, &ctx);
        assert!(load_drc_payment_channel_live(&funded, &channel_id)
            .unwrap()
            .unwrap()
            .close_finalizable_after
            .is_some());

        let close = signed_close(
            &dest,
            channel_id,
            DrcPaymentChannelCloseKind::DestinationClose,
            1,
            &ctx,
        );
        let mut block4 = coinbase(vec![Hash::ZERO], &dest);
        block4.drc_payment_channel_closes.push(close);
        apply_channel_block(&funded, block4, 5, &ctx);
        assert!(load_drc_payment_channel_receipt(&funded, &channel_id)
            .unwrap()
            .is_some());
    }
}
