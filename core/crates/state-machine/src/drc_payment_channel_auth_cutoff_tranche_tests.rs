//! Policy, DepositAuth, tags, cap, and conservation for payment channels.

#[cfg(test)]
mod dest_tag_and_invoice {
    use agora_types::{
        DrcPaymentChannelCreateTx, DrcPaymentChannelError, Hash,
        DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
    };
    use borsh::BorshDeserialize;

    use crate::accounts::load_account;
    use crate::drc_payment_channel::{
        load_drc_payment_channel_live, load_drc_payment_channel_receipt,
    };
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, fund, key, signed_close, signed_create,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::{DrcPaymentChannelCloseKind, NativeAssetId};

    #[test]
    fn some_zero_tag_satisfies_require_dest_tag_and_round_trips() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let destination = key(2);
        let claim_key = key(3);
        fund(&store, &owner, 5_000);
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

        let create = signed_create(
            &owner,
            &claim_key,
            destination.address(),
            25,
            0,
            &ctx,
            Some(0),
            None,
            None,
            5,
        );
        let id = create.channel_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_creates.push(create.clone());
        apply_channel_block(&store, block, 1, &ctx);
        let live = load_drc_payment_channel_live(&store, &id).unwrap().unwrap();
        assert_eq!(live.destination_tag, Some(0));

        fund(&store, &destination, 5);
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let close = signed_close(
            &destination,
            id,
            DrcPaymentChannelCloseKind::DestinationClose,
            dest_nonce,
            &ctx,
        );
        let mut block2 = coinbase(vec![Hash::ZERO], &destination);
        block2.drc_payment_channel_closes.push(close);
        apply_channel_block(&store, block2, 2, &ctx);
        let receipt = load_drc_payment_channel_receipt(&store, &id)
            .unwrap()
            .unwrap();
        assert_eq!(receipt.destination_tag, Some(0));

        let bytes = borsh::to_vec(&create).unwrap();
        let decoded = DrcPaymentChannelCreateTx::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.destination_tag, Some(0));
    }

    #[test]
    fn nonzero_invoice_rejected_without_state_mutation() {
        let store = StateStore::open_in_memory();
        let owner = key(4);
        fund(&store, &owner, 1_000);
        let before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let tx = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
            owner: owner.address(),
            destination: key(5).address(),
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            claim_public_key: key(6).public_key_bytes().to_vec(),
            settle_delay_blue_scores: 5,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash([9; 32]),
            cancel_after_blue_score: None,
            nonce: before.nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(matches!(
            tx.validate_structure(),
            Err(DrcPaymentChannelError::NonZeroInvoiceNotSupported)
        ));
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap(),
            before
        );
    }
}

#[cfg(test)]
mod deposit_auth_claims {
    use crate::accounts::load_account;
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, create_live_channel, fund, key, signed_claim,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    #[test]
    fn deposit_auth_does_not_block_destination_signed_claim() {
        let store = StateStore::open_in_memory();
        let owner = key(10);
        let dest = key(11);
        let claim_key = key(12);
        fund(&store, &owner, 2_000);
        fund(&store, &dest, 10);
        let ctx = auth();
        let mut pol = agora_types::DrcAccountPolicyTx::set_deposit_auth_required(
            dest.address(),
            agora_types::Amount::ZERO,
            0,
        );
        agora_crypto::sign_drc_account_policy_bound(&mut pol, &dest, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(&store, &pol, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();

        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 80, 0, &ctx);
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &dest.address())
            .unwrap()
            .nonce;
        let claim = signed_claim(&dest, &claim_key, channel_id, 20, dest_nonce, &ctx);
        let mut block = coinbase(vec![agora_types::Hash::ZERO], &dest);
        block.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block, 2, &ctx);
        assert!(
            load_account(&store, NativeAssetId::DRC, &dest.address())
                .unwrap()
                .balance
                > 10
        );
    }
}

#[cfg(test)]
mod conservation {
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, create_live_channel, fund, key, spendable_plus_locked,
    };
    use crate::StateStore;

    #[test]
    fn create_and_fund_fees_reduce_aggregate_spendable_plus_locked() {
        let store = StateStore::open_in_memory();
        let owner = key(20);
        let dest = key(21);
        let claim_key = key(22);
        fund(&store, &owner, 10_000);
        fund(&store, &dest, 50);
        let ctx = auth();
        let baseline = spendable_plus_locked(&store, &owner, &dest);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 200, 0, &ctx);
        assert_eq!(spendable_plus_locked(&store, &owner, &dest), baseline - 1);
        let mut block = coinbase(vec![agora_types::Hash::ZERO], &owner);
        block.drc_payment_channel_funds.push(
            crate::drc_payment_channel_test_harness::support::signed_fund(
                &owner, channel_id, 50, 1, &ctx,
            ),
        );
        apply_channel_block(&store, block, 2, &ctx);
        assert_eq!(spendable_plus_locked(&store, &owner, &dest), baseline - 2);
    }
}
