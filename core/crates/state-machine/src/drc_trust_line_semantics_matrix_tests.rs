//! Exact semantics and rejection preservation for trust lines and issued transfers.

#[cfg(test)]
mod tests {
    use agora_types::{Hash, NativeAssetId};

    use crate::drc_trust_line_test_harness::invariants;
    use crate::drc_trust_line_test_harness::support::{
        auth, coinbase, fund, key, setup_live_line, signed_issued_transfer, signed_trust_line_set,
        std_code,
    };
    use crate::StateStore;

    fn reject(
        store: &StateStore,
        block: agora_types::Block,
        issuer: &agora_crypto::KeyPair,
        cur: agora_types::IssuedCurrencyCode,
        holders: &[&agora_crypto::KeyPair],
        tickets: &[&agora_crypto::KeyPair],
    ) {
        let ctx = auth();
        invariants::reject_block_preserving(store, &block, &ctx, issuer, cur, holders, tickets);
    }

    #[test]
    fn matrix_reject_issue_without_recipient_line() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let holder = key(2);
        fund(&store, &issuer, 100);
        let cur = std_code(b"M01");
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[]);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            1,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        reject(&store, block, &issuer, cur, &[&holder], &[]);
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[], &before);
    }

    #[test]
    fn matrix_reject_issue_exceeds_limit() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(3);
        let holder = key(4);
        let cur = std_code(b"M02");
        setup_live_line(&store, &holder, &issuer, cur, 5, 1);
        fund(&store, &issuer, 100);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            10,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        reject(&store, block, &issuer, cur, &[&holder], &[]);
    }

    #[test]
    fn matrix_reject_holder_transfer_missing_recipient_line() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(5);
        let a = key(6);
        let b = key(7);
        let cur = std_code(b"M03");
        setup_live_line(&store, &a, &issuer, cur, 50, 1);
        fund(&store, &issuer, 100);
        fund(&store, &a, 100);
        let mut ib = coinbase(vec![Hash::ZERO], &issuer);
        ib.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            a.address(),
            &issuer,
            cur,
            10,
            1,
            0,
            &ctx,
        ));
        ib.header.tx_root = ib.compute_body_root();
        crate::drc_trust_line_test_harness::support::apply_block(&store, ib, 2, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &a);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &a,
            b.address(),
            &issuer,
            cur,
            3,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        reject(&store, block, &issuer, cur, &[&a, &b], &[]);
    }

    #[test]
    fn matrix_reject_zero_amount_transfer() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(8);
        let holder = key(9);
        let cur = std_code(b"M04");
        setup_live_line(&store, &holder, &issuer, cur, 10, 1);
        fund(&store, &issuer, 50);
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[]);
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx.amount = agora_types::IssuedAmount::ZERO;
        assert!(tx.validate_structure().is_err());
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[], &before);
    }

    #[test]
    fn matrix_reject_wrong_issuer_field() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(10);
        let wrong = key(11);
        let holder = key(12);
        let cur = std_code(b"M05");
        setup_live_line(&store, &holder, &issuer, cur, 20, 1);
        fund(&store, &wrong, 50);
        let mut tx = signed_issued_transfer(&wrong, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx.issuer = wrong.address();
        agora_crypto::sign_drc_issued_transfer_bound(&mut tx, &wrong, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &wrong);
        block.drc_issued_transfers.push(tx);
        block.header.tx_root = block.compute_body_root();
        reject(&store, block, &issuer, cur, &[&holder], &[]);
    }

    #[test]
    fn matrix_reject_self_trust_line() {
        let store = StateStore::open_in_memory();
        let acct = key(13);
        fund(&store, &acct, 50);
        let cur = std_code(b"M06");
        let before = invariants::snapshot(&store, &acct, cur, &[&acct], &[]);
        let tx = agora_types::DrcTrustLineSetTx {
            version: agora_types::DRC_TRUST_LINE_SET_TX_VERSION,
            holder: acct.address(),
            issuer: acct.address(),
            currency: cur,
            limit: agora_types::IssuedAmount::from_units(10),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert_eq!(
            tx.validate_structure(),
            Err(agora_types::DrcTrustLineError::SelfTrustLine)
        );
        invariants::assert_unchanged(&store, &acct, cur, &[&acct], &[], &before);
    }

    #[test]
    fn matrix_delete_nonzero_balance_rejected() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(14);
        let holder = key(15);
        let cur = std_code(b"M07");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 100);
        let mut ib = coinbase(vec![Hash::ZERO], &issuer);
        ib.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            5,
            1,
            0,
            &ctx,
        ));
        ib.header.tx_root = ib.compute_body_root();
        crate::drc_trust_line_test_harness::support::apply_block(&store, ib, 2, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 0, 1, 1, &ctx));
        block.header.tx_root = block.compute_body_root();
        reject(&store, block, &issuer, cur, &[&holder], &[]);
    }

    #[test]
    fn matrix_reject_preserves_native_balances_on_failed_fee_debit() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(16);
        let holder = key(17);
        fund(&store, &holder, 10);
        fund(&store, &issuer, 50);
        let cur = std_code(b"M08");
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[]);
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 10, 100, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        reject(&store, block, &issuer, cur, &[&holder], &[]);
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[], &before);
        let _ = NativeAssetId::DRC;
    }
}
