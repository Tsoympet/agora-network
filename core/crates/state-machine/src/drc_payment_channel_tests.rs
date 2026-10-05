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
