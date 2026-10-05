//! Native DRC payment channel consensus-core tests (no public admission).

#[cfg(test)]
mod tests {
    use agora_crypto::verify_payment_channel_offledger_claim;
    use agora_types::{
        payment_channel_finalize_allowed, Amount, DrcPaymentChannelCloseKind, Hash, NativeAssetId,
    };

    use crate::accounts::load_account;
    use crate::drc_payment_channel::{
        load_drc_payment_channel_live, load_drc_payment_channel_receipt,
        lookup_drc_payment_channel_point,
    };
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, create_live_channel, fund, key, signed_claim,
        signed_close, signed_fund,
    };
    use crate::StateStore;

    #[test]
    fn create_locks_initial_amount_plus_fee() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(1);
        let dest = key(2);
        let claim_key = key(42);
        fund(&store, &owner, 200);
        let before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        let after = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(after.balance, before.balance - 101);
        let live = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap();
        assert_eq!(live.total_funded, Amount::from_base_units(100));
    }

    #[test]
    fn fund_increases_total_without_invalidating_prior_offledger_claim() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(10);
        let dest = key(11);
        let claim_key = key(55);
        fund(&store, &owner, 500);
        fund(&store, &dest, 10);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);

        let cumulative = 40u64;
        let sig = agora_crypto::sign_payment_channel_offledger_claim(
            &claim_key,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            Amount::from_base_units(cumulative),
        )
        .unwrap();
        verify_payment_channel_offledger_claim(
            &claim_key.public_key_bytes(),
            &sig,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            Amount::from_base_units(cumulative),
        )
        .unwrap();

        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block
            .drc_payment_channel_funds
            .push(signed_fund(&owner, channel_id, 50, 1, &ctx));
        apply_channel_block(&store, block, 2, &ctx);

        verify_payment_channel_offledger_claim(
            &claim_key.public_key_bytes(),
            &sig,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            Amount::from_base_units(cumulative),
        )
        .unwrap();

        let live = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap();
        assert_eq!(live.total_funded, Amount::from_base_units(150));

        let claim = signed_claim(&dest, &claim_key, channel_id, cumulative, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block, 3, &ctx);
        let live = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap();
        assert_eq!(live.cumulative_claimed, Amount::from_base_units(cumulative));
    }

    #[test]
    fn claim_moves_locked_value_without_owner_liquid_debit() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(30);
        let dest = key(31);
        let claim_key = key(57);
        fund(&store, &owner, 500);
        fund(&store, &dest, 10);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        let owner_before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let dest_before = load_account(&store, NativeAssetId::DRC, &dest.address()).unwrap();
        let locked_before = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap()
            .locked_remainder()
            .unwrap()
            .as_base_units();

        let claim = signed_claim(&dest, &claim_key, channel_id, 25, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block, 2, &ctx);

        let owner_after = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let dest_after = load_account(&store, NativeAssetId::DRC, &dest.address()).unwrap();
        let locked_after = load_drc_payment_channel_live(&store, &channel_id)
            .unwrap()
            .unwrap()
            .locked_remainder()
            .unwrap()
            .as_base_units();

        assert_eq!(owner_after.balance, owner_before.balance);
        assert_eq!(dest_after.balance, dest_before.balance + 25 - 1);
        assert_eq!(locked_after, locked_before - 25);
        assert_eq!(
            owner_after.balance + dest_after.balance + locked_after,
            owner_before.balance + dest_before.balance + locked_before - 1,
            "claim fee is the only spendable+locked loss"
        );
    }

    #[test]
    fn owner_finalize_preserves_close_fee_when_returning_remainder() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(40);
        let dest = key(41);
        let claim_key = key(58);
        fund(&store, &owner, 1_000);
        fund(&store, &dest, 10);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        let claim = signed_claim(&dest, &claim_key, channel_id, 25, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block, 2, &ctx);

        let owner_before_schedule =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let schedule = signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            1,
            &ctx,
        );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(schedule);
        apply_channel_block(&store, block, 3, &ctx);
        let owner_after_schedule =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(
            owner_after_schedule.balance,
            owner_before_schedule.balance - 1
        );

        let finalize = signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::Finalize,
            2,
            &ctx,
        );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_closes.push(finalize);
        apply_channel_block(&store, block, 12, &ctx);
        let owner_after_finalize =
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(
            owner_after_finalize.balance,
            owner_after_schedule.balance - 1 + 75,
            "finalize debits close fee then returns locked remainder (100 - 25)"
        );
    }

    #[test]
    fn destination_close_returns_remainder_once() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(20);
        let dest = key(21);
        let claim_key = key(56);
        fund(&store, &owner, 300);
        fund(&store, &dest, 5);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);

        let claim = signed_claim(&dest, &claim_key, channel_id, 30, 0, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block, 2, &ctx);
        let owner_mid = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();

        let close = signed_close(
            &dest,
            channel_id,
            DrcPaymentChannelCloseKind::DestinationClose,
            1,
            &ctx,
        );
        let mut block = coinbase(vec![Hash::ZERO], &dest);
        block.drc_payment_channel_closes.push(close);
        apply_channel_block(&store, block, 3, &ctx);

        let owner_after = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(owner_after.balance, owner_mid.balance + 70);
        assert_eq!(
            lookup_drc_payment_channel_point(&store, &channel_id).unwrap(),
            "receipt"
        );
        assert!(load_drc_payment_channel_receipt(&store, &channel_id)
            .unwrap()
            .is_some());
    }

    #[test]
    fn owner_schedule_finalize_boundary() {
        assert!(!payment_channel_finalize_allowed(9, Some(10), None));
        assert!(payment_channel_finalize_allowed(10, Some(10), None));
    }
}
