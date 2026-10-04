//! Stage-D: selector, replay, version, and cross-owner ticket guards.

#[cfg(test)]
mod selector_replay_version {
    use agora_crypto::{sign_drc_payment_bound, sign_drc_ticket_create_bound};
    use agora_types::{
        Amount, DrcAccountSequenceSelector, DrcPaymentTx, DrcTicketCreateTx, Hash,
        DRC_PAYMENT_LEGACY_VERSION, DRC_PAYMENT_TICKET_VERSION,
    };

    use crate::apply::{
        apply_block_batched_virtual_at_blue_score, apply_block_batched_with_auth_at_blue_score,
    };
    use crate::drc_mempool::{
        lookup_drc_ticket_point, plan_drc_mempool_reservation, DrcTicketPointStatus,
    };
    use crate::drc_tickets_test_harness::support::{
        auth, coinbase, fund, key, mint_ticket,
        reject_block_preserving_ticket_state, snapshot_ticket_state,
    };
    use crate::store::WriteBatch;
    use crate::{apply_drc_ticket_create, AccountJournal, StateStore};

    #[test]
    fn consumed_ticket_cannot_be_reused_on_second_block() {
        let store = StateStore::open_in_memory();
        let owner = key(20);
        let peer = key(21);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(payment.clone());
        block.header.tx_root = block.compute_body_root();
        store
            .write_batch(
                apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                    .unwrap()
                    .batch,
            )
            .unwrap();
        assert_eq!(
            lookup_drc_ticket_point(&store, &owner.address(), seq).unwrap(),
            DrcTicketPointStatus::Unknown
        );
        payment.nonce = 1;
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut replay = coinbase(vec![block.id()], &owner);
        replay.drc_payments.push(payment);
        replay.header.tx_root = replay.compute_body_root();
        assert!(
            apply_block_batched_with_auth_at_blue_score(&store, &replay, 50, Some(&ctx), 50)
                .is_err()
        );
    }

    #[test]
    fn legacy_payment_envelope_rejects_ticket_selector() {
        let store = StateStore::open_in_memory();
        let owner = key(22);
        fund(&store, &owner, 100);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(23).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_LEGACY_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        assert!(sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).is_err());
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn future_payment_version_rejects_ticket_selector() {
        let store = StateStore::open_in_memory();
        let owner = key(24);
        fund(&store, &owner, 100);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(25).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION + 1;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        assert!(sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).is_err());
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn selector_tamper_after_sign_rejects_and_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(26);
        fund(&store, &owner, 500);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(27).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq + 1));
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn zero_owner_ticket_lookup_rejected() {
        let store = StateStore::open_in_memory();
        assert!(lookup_drc_ticket_point(&store, &agora_types::Address::ZERO, 1).is_err());
    }

    #[test]
    fn overflow_ticket_create_nonce_rejected() {
        let store = StateStore::open_in_memory();
        let owner = key(28);
        fund(&store, &owner, 100);
        let ctx = auth();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, u64::MAX);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(apply_drc_ticket_create(&store, &create, &ctx, &mut batch, &mut journal).is_err());
    }

    #[test]
    fn mempool_rejects_unknown_ticket_sequence() {
        let store = StateStore::open_in_memory();
        let owner = key(29);
        fund(&store, &owner, 100);
        assert!(plan_drc_mempool_reservation(
            &store,
            &owner.address(),
            DRC_PAYMENT_TICKET_VERSION,
            DRC_PAYMENT_TICKET_VERSION,
            0,
            Some(DrcAccountSequenceSelector::ticket(42)),
        )
        .is_err());
    }

    #[test]
    fn cross_owner_ticket_payment_leaves_owner_ticket_live() {
        let store = StateStore::open_in_memory();
        let owner = key(30);
        let attacker = key(31);
        fund(&store, &owner, 500);
        fund(&store, &attacker, 500);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut payment = DrcPaymentTx::unsigned_v4(
            attacker.address(),
            key(32).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut payment, &attacker, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &attacker);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn duplicate_ticket_consumer_virtual_lane_is_deterministic() {
        let store = StateStore::open_in_memory();
        let owner = key(33);
        let peer = key(34);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let mut pay_a = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        pay_a.version = DRC_PAYMENT_TICKET_VERSION;
        pay_a.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut pay_a, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut pay_b = pay_a.clone();
        pay_b.amount = Amount::from_base_units(2);
        sign_drc_payment_bound(&mut pay_b, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(pay_a);
        block.drc_payments.push(pay_b);
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_virtual_at_blue_score(&store, &block, 50, Some(&ctx), 50).unwrap();
        assert_eq!(
            result.acceptance.payment_statuses,
            vec![
                agora_types::TransactionAcceptance::Accepted,
                agora_types::TransactionAcceptance::ConflictLost
            ]
        );
    }
}
