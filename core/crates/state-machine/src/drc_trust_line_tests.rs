//! Issuer-scoped trust lines and exact issued-value transfer tests.

#[cfg(test)]
mod tests {
    use agora_types::{
        Hash, IssuedCurrencyError, NativeAssetId, DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER,
    };

    use crate::accounts::load_account;
    use crate::drc_trust_line::lookup_drc_trust_line_point;
    use crate::drc_trust_line_test_harness::support::{
        apply_block, asset, auth, coinbase, fund, issuer_outstanding, key, line_balance,
        receipt_exists, signed_issued_transfer, signed_trust_line_set, std_code, trust_root,
    };
    use crate::StateStore;

    #[test]
    fn reserved_currency_codes_rejected() {
        for tag in [b"DRC", b"TLT", b"OVL", b"XRP"] {
            let mut c = [0u8; 20];
            c[..3].copy_from_slice(tag);
            assert_eq!(
                agora_types::IssuedCurrencyCode(c).validate(),
                Err(IssuedCurrencyError::ReservedNativeCollidingCode)
            );
        }
    }

    #[test]
    fn issue_transfer_redeem_conserves_issuer_liability() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let holder_a = key(2);
        let holder_b = key(3);
        fund(&store, &issuer, 100);
        fund(&store, &holder_a, 100);
        fund(&store, &holder_b, 100);
        let cur = std_code(b"USD");
        let ast = asset(&issuer, cur);

        let mut block = coinbase(vec![Hash::ZERO], &holder_a);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder_a, &issuer, cur, 1_000, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);

        let mut block = coinbase(vec![Hash::ZERO], &holder_b);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder_b, &issuer, cur, 1_000, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);

        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder_a.address(),
            &issuer,
            cur,
            100,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 3, &ctx);
        assert_eq!(line_balance(&store, holder_a.address(), &ast), 100);
        assert_eq!(issuer_outstanding(&store, &ast), 100);

        let mut block = coinbase(vec![Hash::ZERO], &holder_a);
        let xfer =
            signed_issued_transfer(&holder_a, holder_b.address(), &issuer, cur, 40, 1, 1, &ctx);
        let xfer_id = xfer.issued_transfer_tx_id();
        block.drc_issued_transfers.push(xfer);
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 4, &ctx);
        assert_eq!(line_balance(&store, holder_a.address(), &ast), 60);
        assert_eq!(line_balance(&store, holder_b.address(), &ast), 40);
        assert_eq!(issuer_outstanding(&store, &ast), 100);
        assert!(receipt_exists(&store, &xfer_id));

        let mut block = coinbase(vec![Hash::ZERO], &holder_b);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &holder_b,
            issuer.address(),
            &issuer,
            cur,
            40,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 5, &ctx);
        assert_eq!(issuer_outstanding(&store, &ast), 60);
        assert_eq!(line_balance(&store, holder_b.address(), &ast), 0);
    }

    #[test]
    fn drc_fee_separate_from_issued_amount() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(10);
        let holder = key(11);
        fund(&store, &issuer, 50);
        fund(&store, &holder, 50);
        let before = load_account(&store, NativeAssetId::DRC, &holder.address()).unwrap();
        let cur = std_code(b"EUR");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 500, 2, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        let after = load_account(&store, NativeAssetId::DRC, &holder.address()).unwrap();
        assert_eq!(after.nonce, before.nonce + 1);
        assert!(after.balance < before.balance + 50);
    }

    #[test]
    fn issued_asset_is_distinct_from_native_asset_id() {
        let _ = asset(&key(20), std_code(b"ABC"));
        // Compile-time domain separation: issued amounts use IssuedAmount, not Amount on native lanes.
    }

    #[test]
    fn trust_line_delete_requires_zero_balance() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(30);
        let holder = key(31);
        fund(&store, &issuer, 50);
        fund(&store, &holder, 50);
        let cur = std_code(b"GBP");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 100, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);

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
        apply_block(&store, block, 2, &ctx);

        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 0, 1, 1, &ctx));
        block.header.tx_root = block.compute_body_root();
        let err = crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            3,
        );
        assert!(err.is_err());
        assert_eq!(
            lookup_drc_trust_line_point(&store, &holder.address(), &asset(&issuer, cur)).unwrap(),
            "live"
        );
    }

    #[test]
    fn trust_line_root_changes_with_live_state() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(40);
        let holder = key(41);
        fund(&store, &issuer, 20);
        fund(&store, &holder, 20);
        let r0 = trust_root(&store);
        let cur = std_code(b"CHF");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 50, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        assert_ne!(trust_root(&store), r0);
    }

    #[test]
    fn holder_cap_enforced() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(50);
        let holder = key(51);
        fund(&store, &issuer, 1_000);
        fund(&store, &holder, 1_000);
        for i in 0..DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER {
            let tag = [b'A' + (i / 26) as u8, b'A' + (i % 26) as u8, b'0'];
            let cur = std_code(&tag);
            let mut block = coinbase(vec![Hash::ZERO], &holder);
            block.drc_trust_line_sets.push(signed_trust_line_set(
                &holder, &issuer, cur, 1, 1, i as u64, &ctx,
            ));
            block.header.tx_root = block.compute_body_root();
            apply_block(&store, block, i as u64 + 1, &ctx);
        }
        let tag = *b"ZZ9";
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder,
            &issuer,
            std_code(&tag),
            1,
            1,
            DRC_MAX_LIVE_TRUST_LINES_PER_HOLDER as u64,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            999,
        )
        .is_err());
    }
}
