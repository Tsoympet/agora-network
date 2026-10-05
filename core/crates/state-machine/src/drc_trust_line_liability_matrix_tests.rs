//! Multi-holder multi-asset liability invariants through success, failure, and journal paths.

#[cfg(test)]
mod tests {
    use agora_types::Hash;

    use crate::accounts::load_account;
    use crate::drc_trust_line::{
        count_live_trust_line_holders_for_issuer, sum_holder_balances_for_asset,
    };
    use crate::drc_trust_line_test_harness::support::{
        apply_block, apply_block_journal, asset, auth, coinbase, fund, issuer_outstanding, key,
        line_balance, revert_journal, setup_live_line, signed_issued_transfer,
        signed_trust_line_set, std_code,
    };
    use crate::StateStore;
    use agora_types::NativeAssetId;

    #[test]
    fn matrix_liability_tracks_issue_transfer_redeem_delete_recreate() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let a = key(2);
        let b = key(3);
        let cur = std_code(b"L01");
        let ast = asset(&issuer, cur);
        setup_live_line(&store, &a, &issuer, cur, 100, 1);
        setup_live_line(&store, &b, &issuer, cur, 100, 2);
        fund(&store, &issuer, 500);
        fund(&store, &a, 50);
        fund(&store, &b, 50);
        let addrs = [issuer.address(), a.address(), b.address()];
        let native0: u64 = addrs
            .iter()
            .map(|ad| {
                load_account(&store, NativeAssetId::DRC, ad)
                    .unwrap()
                    .balance
            })
            .sum();

        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            a.address(),
            &issuer,
            cur,
            40,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 3, &ctx);
        assert_eq!(
            sum_holder_balances_for_asset(&store, &ast)
                .unwrap()
                .as_units(),
            40
        );
        assert_eq!(issuer_outstanding(&store, &ast), 40);

        let mut block = coinbase(vec![Hash::ZERO], &a);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &a,
            b.address(),
            &issuer,
            cur,
            15,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 4, &ctx);
        assert_eq!(issuer_outstanding(&store, &ast), 40);
        assert_eq!(line_balance(&store, a.address(), &ast), 25);
        assert_eq!(line_balance(&store, b.address(), &ast), 15);

        let mut block = coinbase(vec![Hash::ZERO], &b);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &b,
            issuer.address(),
            &issuer,
            cur,
            15,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 5, &ctx);
        assert_eq!(issuer_outstanding(&store, &ast), 25);

        let mut block = coinbase(vec![Hash::ZERO], &b);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&b, &issuer, cur, 0, 1, 2, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 6, &ctx);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            1
        );

        let mut block = coinbase(vec![Hash::ZERO], &b);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&b, &issuer, cur, 10, 1, 3, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 7, &ctx);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            2
        );

        let native1: u64 = addrs
            .iter()
            .map(|ad| {
                load_account(&store, NativeAssetId::DRC, ad)
                    .unwrap()
                    .balance
            })
            .sum();
        assert!(native1 < native0);
    }

    #[test]
    fn matrix_failed_op_journal_revert_restores_liability() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(10);
        let holder = key(11);
        let cur = std_code(b"L02");
        let ast = asset(&issuer, cur);
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
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
        let journal = apply_block_journal(&store, block, 2, &ctx);
        assert_eq!(issuer_outstanding(&store, &ast), 10);
        revert_journal(&store, &journal);
        assert_eq!(issuer_outstanding(&store, &ast), 0);
        assert_eq!(
            sum_holder_balances_for_asset(&store, &ast)
                .unwrap()
                .as_units(),
            0
        );
    }
}
