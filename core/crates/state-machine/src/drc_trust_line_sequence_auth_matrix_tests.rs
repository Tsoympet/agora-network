//! Nonce and Ticket authorization matrices for TrustLineSet and IssuedTransfer.

#[cfg(test)]
mod shared {
    pub use crate::drc_trust_line_test_harness::support::{
        apply_block, auth, coinbase, fund, key, mint_ticket, setup_live_line,
        signed_issued_transfer, signed_trust_line_set, std_code,
    };
}

#[cfg(test)]
mod trust_line_set_ticket_matrix {
    use agora_crypto::sign_drc_trust_line_set_bound;
    use agora_types::{
        DrcAccountSequenceSelector, Hash, NativeAssetId, DRC_TRUST_LINE_SET_TICKET_VERSION,
        DRC_TRUST_LINE_SET_TX_VERSION,
    };

    use crate::accounts::load_account;
    use crate::drc_mempool::{lookup_drc_ticket_point, DrcTicketPointStatus};
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::drc_trust_line_test_harness::invariants;
    use crate::drc_trust_line_test_harness::support::{
        auth, coinbase, fund, key, mint_ticket, signed_trust_line_set, std_code,
    };
    use crate::StateStore;

    use super::shared::apply_block;

    #[test]
    fn matrix_trust_line_set_ordinary_nonce_success_advances_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(1);
        let issuer = key(2);
        fund(&store, &holder, 50);
        fund(&store, &issuer, 50);
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder,
            &issuer,
            std_code(b"A01"),
            10,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &holder.address())
                .unwrap()
                .nonce,
            1
        );
    }

    #[test]
    fn matrix_trust_line_set_ticket_success_one_use_preserves_ordinary_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(3);
        let issuer = key(4);
        fund(&store, &holder, 50);
        fund(&store, &issuer, 50);
        let ticket = mint_ticket(&store, &holder);
        let nonce_before = load_account(&store, NativeAssetId::DRC, &holder.address())
            .unwrap()
            .nonce;
        let mut tx = signed_trust_line_set(&holder, &issuer, std_code(b"T01"), 10, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_SET_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_trust_line_set_bound(&mut tx, &holder, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &holder.address())
                .unwrap()
                .nonce,
            nonce_before
        );
        assert!(!load_drc_account_tickets(&store, &holder.address())
            .unwrap()
            .contains(&ticket));
    }

    #[test]
    fn matrix_trust_line_set_unknown_ticket_rejected_preserves_tickets() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(5);
        let issuer = key(6);
        fund(&store, &holder, 50);
        fund(&store, &issuer, 50);
        let cur = std_code(b"U01");
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[&holder]);
        let mut tx = signed_trust_line_set(&holder, &issuer, cur, 10, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_SET_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(9999));
        sign_drc_trust_line_set_bound(&mut tx, &holder, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        invariants::reject_block_preserving(
            &store,
            &block,
            &ctx,
            &issuer,
            cur,
            &[&holder],
            &[&holder],
        );
        assert_eq!(
            lookup_drc_ticket_point(&store, &holder.address(), 9999).unwrap(),
            DrcTicketPointStatus::Unknown
        );
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[&holder], &before);
    }

    #[test]
    fn matrix_trust_line_set_legacy_version_rejects_ticket_selector() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(7);
        let issuer = key(8);
        fund(&store, &holder, 50);
        fund(&store, &issuer, 50);
        let _ticket = mint_ticket(&store, &holder);
        let cur = std_code(b"L01");
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[&holder]);
        let mut tx = signed_trust_line_set(&holder, &issuer, cur, 5, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_SET_TX_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(_ticket));
        sign_drc_trust_line_set_bound(&mut tx, &holder, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        invariants::reject_block_preserving(
            &store,
            &block,
            &ctx,
            &issuer,
            cur,
            &[&holder],
            &[&holder],
        );
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[&holder], &before);
    }

    #[test]
    fn matrix_trust_line_set_ticket_retained_on_semantic_failure() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(9);
        let issuer = key(10);
        fund(&store, &holder, 50);
        fund(&store, &issuer, 50);
        let ticket = mint_ticket(&store, &holder);
        let cur = std_code(b"F01");
        let mut tx = signed_trust_line_set(&holder, &issuer, cur, 0, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_SET_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_trust_line_set_bound(&mut tx, &holder, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            50
        )
        .is_err());
        assert_eq!(
            lookup_drc_ticket_point(&store, &holder.address(), ticket).unwrap(),
            DrcTicketPointStatus::Live
        );
    }
}

#[cfg(test)]
mod issued_transfer_ticket_matrix {
    use agora_crypto::sign_drc_issued_transfer_bound;
    use agora_types::{
        DrcAccountSequenceSelector, Hash, NativeAssetId,
        DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION, DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
    };

    use crate::accounts::load_account;
    use crate::drc_mempool::{lookup_drc_ticket_point, DrcTicketPointStatus};
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::drc_trust_line_test_harness::invariants;
    use crate::drc_trust_line_test_harness::support::{
        auth, coinbase, fund, key, mint_ticket, setup_live_line, signed_issued_transfer, std_code,
    };
    use crate::StateStore;

    use super::shared::apply_block;

    #[test]
    fn matrix_issued_transfer_ticket_success_preserves_sender_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(20);
        let holder = key(21);
        let cur = std_code(b"I01");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        fund(&store, &issuer, 100);
        let ticket = mint_ticket(&store, &issuer);
        let nonce_before = load_account(&store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 5, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_issued_transfer_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(tx);
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &issuer.address())
                .unwrap()
                .nonce,
            nonce_before
        );
        assert!(!load_drc_account_tickets(&store, &issuer.address())
            .unwrap()
            .contains(&ticket));
    }

    #[test]
    fn matrix_issued_transfer_reused_ticket_rejected() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(22);
        let holder = key(23);
        let cur = std_code(b"I02");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 100);
        let ticket = mint_ticket(&store, &issuer);
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_issued_transfer_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(tx);
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        let mut tx2 =
            signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx2.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION;
        tx2.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_issued_transfer_bound(&mut tx2, &issuer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block2 = coinbase(vec![Hash::ZERO], &issuer);
        block2.drc_issued_transfers.push(tx2);
        block2.header.tx_root = block2.compute_body_root();
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block2,
            50,
            Some(&ctx),
            3
        )
        .is_err());
    }

    #[test]
    fn matrix_issued_transfer_cross_owner_ticket_rejected_preserves_victim_ticket() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(24);
        let holder = key(25);
        let attacker = key(26);
        let cur = std_code(b"I03");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 100);
        fund(&store, &attacker, 100);
        let victim_ticket = mint_ticket(&store, &issuer);
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[&issuer]);
        let mut tx =
            signed_issued_transfer(&attacker, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(victim_ticket));
        sign_drc_issued_transfer_bound(&mut tx, &attacker, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &attacker);
        block.drc_issued_transfers.push(tx);
        block.header.tx_root = block.compute_body_root();
        invariants::reject_block_preserving(
            &store,
            &block,
            &ctx,
            &issuer,
            cur,
            &[&holder],
            &[&issuer],
        );
        assert_eq!(
            lookup_drc_ticket_point(&store, &issuer.address(), victim_ticket).unwrap(),
            DrcTicketPointStatus::Live
        );
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[&issuer], &before);
    }

    #[test]
    fn matrix_issued_transfer_legacy_version_rejects_ticket_selector() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(27);
        let holder = key(28);
        let cur = std_code(b"I04");
        setup_live_line(&store, &holder, &issuer, cur, 20, 1);
        fund(&store, &issuer, 50);
        let ticket = mint_ticket(&store, &issuer);
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_issued_transfer_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(tx);
        block.header.tx_root = block.compute_body_root();
        invariants::reject_block_preserving(
            &store,
            &block,
            &ctx,
            &issuer,
            cur,
            &[&holder],
            &[&issuer],
        );
    }
}
