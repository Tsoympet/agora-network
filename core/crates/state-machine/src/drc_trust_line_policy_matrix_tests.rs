//! Policy composition matrix for issued transfers (RequireDestTag, DepositAuth, invoice).

#[cfg(test)]
mod tests {
    use agora_types::Hash;

    use crate::drc_trust_line::load_drc_issued_transfer_receipt;
    use crate::drc_trust_line_test_harness::invariants;
    use crate::drc_trust_line_test_harness::policy::{
        clear_require_dest_tag, grant_deposit_preauth, set_deposit_auth, set_require_dest_tag,
    };
    use crate::drc_trust_line_test_harness::support::{
        apply_block, auth, coinbase, fund, key, setup_live_line, signed_issued_transfer, std_code,
    };
    use crate::StateStore;

    #[test]
    fn matrix_require_dest_tag_some_zero_valid_on_issue() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let holder = key(2);
        let cur = std_code(b"P01");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        fund(&store, &issuer, 100);
        set_require_dest_tag(&store, &holder, &ctx);
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 3, 1, 0, &ctx);
        tx.destination_tag = Some(0);
        agora_crypto::sign_drc_issued_transfer_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let tx_id = tx.issued_transfer_tx_id();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(tx);
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        let receipt = load_drc_issued_transfer_receipt(&store, &tx_id)
            .unwrap()
            .unwrap();
        assert_eq!(receipt.destination_tag, Some(0));
    }

    #[test]
    fn matrix_require_dest_tag_missing_rejected_atomically() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(3);
        let holder = key(4);
        let cur = std_code(b"P02");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 100);
        set_require_dest_tag(&store, &holder, &ctx);
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[]);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            2,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        invariants::reject_block_preserving(&store, &block, &ctx, &issuer, cur, &[&holder], &[]);
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[], &before);
    }

    #[test]
    fn matrix_deposit_auth_issue_requires_preauth() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(5);
        let holder = key(6);
        let cur = std_code(b"P03");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 100);
        set_deposit_auth(&store, &holder, &ctx);
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
        invariants::reject_block_preserving(&store, &block, &ctx, &issuer, cur, &[&holder], &[]);
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[], &before);
        grant_deposit_preauth(&store, &holder, issuer.address(), &ctx);
        let mut block2 = coinbase(vec![Hash::ZERO], &issuer);
        block2.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            1,
            1,
            0,
            &ctx,
        ));
        block2.header.tx_root = block2.compute_body_root();
        apply_block(&store, block2, 2, &ctx);
    }

    #[test]
    fn matrix_nonzero_invoice_rejected() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(7);
        let holder = key(8);
        let cur = std_code(b"P04");
        setup_live_line(&store, &holder, &issuer, cur, 20, 1);
        fund(&store, &issuer, 50);
        let before = invariants::snapshot(&store, &issuer, cur, &[&holder], &[]);
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 1, 1, 0, &ctx);
        tx.invoice_id = Hash([1; 32]);
        assert!(tx.validate_structure().is_err());
        invariants::assert_unchanged(&store, &issuer, cur, &[&holder], &[], &before);
    }

    #[test]
    fn matrix_policy_toggle_require_dest_tag() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(9);
        fund(&store, &holder, 10);
        set_require_dest_tag(&store, &holder, &ctx);
        clear_require_dest_tag(&store, &holder, &ctx);
        assert!(
            !crate::drc_policy::load_drc_account_policy(&store, &holder.address())
                .unwrap()
                .require_destination_tag
        );
    }
}
