//! Cap, liability, policy, same-block order, and semantics hardening for trust lines.

#[cfg(test)]
mod tests {
    use agora_types::{
        Hash, IssuedAmount, NativeAssetId, TransactionAcceptance,
        DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER, DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER,
    };

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_with_auth_at_blue_score;
    use crate::drc_trust_line::{
        count_live_trust_line_holders_for_issuer, count_live_trust_lines_for_holder,
        load_drc_issued_transfer_receipt, sum_holder_balances_for_asset,
    };
    use crate::drc_trust_line_test_harness::support::{
        apply_block, apply_block_journal, asset, auth, coinbase, fund, issuer_outstanding, key,
        line_balance, revert_journal, signed_issued_transfer, signed_trust_line_set, std_code,
        trust_root,
    };
    use crate::StateStore;

    fn unique_holder(i: usize) -> agora_crypto::KeyPair {
        let mut sk = [0u8; 32];
        sk[..8].copy_from_slice(&(i as u64).to_le_bytes());
        sk[31] = 0x5A;
        agora_crypto::KeyPair::from_secret_bytes(&sk).unwrap()
    }

    #[test]
    fn issuer_holder_cap_exact_and_overflow() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        fund(&store, &issuer, 10_000);
        let cur = std_code(b"CAP");
        for i in 0..DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER {
            let holder = unique_holder(i + 1000);
            fund(&store, &holder, 10);
            let mut block = coinbase(vec![Hash::ZERO], &holder);
            block
                .drc_trust_line_sets
                .push(signed_trust_line_set(&holder, &issuer, cur, 1, 1, 0, &ctx));
            block.header.tx_root = block.compute_body_root();
            apply_block(&store, block, i as u64 + 1, &ctx);
        }
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            DRC_MAX_TRUST_LINE_HOLDERS_PER_ISSUER
        );
        let extra = key(99);
        fund(&store, &extra, 10);
        let mut block = coinbase(vec![Hash::ZERO], &extra);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&extra, &issuer, cur, 1, 1, 999, &ctx));
        block.header.tx_root = block.compute_body_root();
        assert!(
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 5000,)
                .is_err()
        );
    }

    #[test]
    fn update_at_holder_cap_does_not_increment_counts() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(10);
        let holder = key(11);
        fund(&store, &issuer, 500);
        fund(&store, &holder, 500);
        for i in 0..DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER {
            let tag = [b'A' + (i / 26) as u8, b'A' + (i % 26) as u8, b'1'];
            let mut block = coinbase(vec![Hash::ZERO], &holder);
            block.drc_trust_line_sets.push(signed_trust_line_set(
                &holder,
                &issuer,
                std_code(&tag),
                1,
                1,
                i as u64,
                &ctx,
            ));
            block.header.tx_root = block.compute_body_root();
            apply_block(&store, block, i as u64 + 1, &ctx);
        }
        assert_eq!(
            count_live_trust_lines_for_holder(&store, &holder.address()).unwrap(),
            DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER
        );
        let tag = *b"AA1";
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder,
            &issuer,
            std_code(&tag),
            2,
            1,
            DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER as u64,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 9000, &ctx);
        assert_eq!(
            count_live_trust_lines_for_holder(&store, &holder.address()).unwrap(),
            DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER
        );
    }

    #[test]
    fn delete_recreate_and_issuer_index_decrement() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(20);
        let holder = key(21);
        fund(&store, &issuer, 100);
        fund(&store, &holder, 100);
        let cur = std_code(b"DEL");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 50, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            1
        );
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 0, 1, 1, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            0
        );
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 10, 1, 2, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 3, &ctx);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            1
        );
    }

    #[test]
    fn liability_equals_sum_of_holder_balances() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(30);
        let a = key(31);
        let b = key(32);
        fund(&store, &issuer, 200);
        fund(&store, &a, 50);
        fund(&store, &b, 50);
        let cur = std_code(b"LIQ");
        let ast = asset(&issuer, cur);
        for holder in [&a, &b] {
            let mut block = coinbase(vec![Hash::ZERO], holder);
            block.drc_trust_line_sets.push(signed_trust_line_set(
                holder, &issuer, cur, 1000, 1, 0, &ctx,
            ));
            block.header.tx_root = block.compute_body_root();
            apply_block(&store, block, holder.address().0[0] as u64 + 1, &ctx);
        }
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
        apply_block(&store, block, 10, &ctx);
        assert_eq!(
            sum_holder_balances_for_asset(&store, &ast)
                .unwrap()
                .as_units(),
            issuer_outstanding(&store, &ast)
        );
    }

    #[test]
    fn same_block_create_then_transfer_succeeds_delete_then_transfer_conflicts() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(40);
        let holder = key(41);
        fund(&store, &issuer, 100);
        fund(&store, &holder, 100);
        let cur = std_code(b"SMB");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 100, 1, 0, &ctx,
        ));
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            5,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        assert_eq!(
            line_balance(&store, holder.address(), &asset(&issuer, cur)),
            5
        );

        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &holder,
            issuer.address(),
            &issuer,
            cur,
            5,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        assert_eq!(
            line_balance(&store, holder.address(), &asset(&issuer, cur)),
            0
        );

        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 0, 1, 2, &ctx));
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            1,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 3).unwrap();
        assert_eq!(
            result.acceptance.drc_issued_transfer_statuses[0],
            TransactionAcceptance::ConflictLost
        );
    }

    #[test]
    fn receipt_preserves_tags_and_participants() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(50);
        let holder = key(51);
        fund(&store, &issuer, 100);
        fund(&store, &holder, 100);
        let cur = std_code(b"TAG");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 100, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        let mut tx = signed_issued_transfer(&issuer, holder.address(), &issuer, cur, 3, 1, 0, &ctx);
        tx.source_tag = Some(7);
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
        assert_eq!(receipt.source_tag, Some(7));
        assert_eq!(receipt.destination_tag, Some(0));
        assert_eq!(receipt.amount, IssuedAmount::from_units(3));
    }

    #[test]
    fn rollback_restores_cap_counts_and_root() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(60);
        let holder = key(61);
        fund(&store, &issuer, 50);
        fund(&store, &holder, 50);
        let root0 = trust_root(&store);
        let cur = std_code(b"RBK");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 10, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        let journal = apply_block_journal(&store, block, 1, &ctx);
        assert_ne!(trust_root(&store), root0);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            1
        );
        revert_journal(&store, &journal);
        assert_eq!(trust_root(&store), root0);
        assert_eq!(
            count_live_trust_line_holders_for_issuer(&store, &issuer.address(), &cur).unwrap(),
            0
        );
    }

    #[test]
    fn native_drc_supply_unchanged_by_issued_flow_except_fees() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(70);
        let holder = key(71);
        fund(&store, &issuer, 100);
        fund(&store, &holder, 100);
        let addrs = [issuer.address(), holder.address()];
        let before: u64 = addrs
            .iter()
            .map(|a| load_account(&store, NativeAssetId::DRC, a).unwrap().balance)
            .sum();
        let cur = std_code(b"FEE");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 50, 2, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            10,
            2,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        let after: u64 = addrs
            .iter()
            .map(|a| load_account(&store, NativeAssetId::DRC, a).unwrap().balance)
            .sum();
        assert!(after < before);
    }
}
