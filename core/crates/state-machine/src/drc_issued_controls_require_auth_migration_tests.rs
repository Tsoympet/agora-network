//! RequireAuth migration: v1 lines must not bypass authorization after policy enable.

#[cfg(test)]
mod tests {
    use agora_types::{
        DrcIssuedAssetPolicyAction, DRC_TRUST_LINE_LIVE_STATE_V2, DRC_TRUST_LINE_LIVE_STATE_VERSION,
    };

    use crate::drc_issued_controls_test_harness::support::{
        apply_block, apply_policy_direct, asset, auth, coinbase, fund, issuer_drc_nonce, key,
        line_on_disk_authorized, line_on_disk_version, persist_legacy_v1_trust_line_on_disk,
        setup_live_line, signed_policy_set, signed_trust_line_set, std_code,
    };
    use crate::StateStore;
    use agora_types::Hash;

    #[test]
    fn enable_require_auth_persists_v2_unauthorized_on_existing_v1_line() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xA1);
        let holder = key(0xA2);
        let cur = std_code(b"RA1");
        let ast = asset(&issuer, cur);
        fund(&store, &issuer, 10_000);
        fund(&store, &holder, 100);
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 500, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        persist_legacy_v1_trust_line_on_disk(&store, holder.address(), &ast);
        assert_eq!(
            line_on_disk_version(&store, holder.address(), &ast),
            DRC_TRUST_LINE_LIVE_STATE_VERSION
        );

        let nonce_before = issuer_drc_nonce(&store, &issuer);
        let pol = signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableRequireAuth,
            1,
            nonce_before,
            &ctx,
        );
        apply_policy_direct(&store, &pol, &ctx, 2).unwrap();
        assert!(
            load_drc_issued_asset_policy(&store, &ast)
                .unwrap()
                .require_auth
        );
        assert_eq!(
            line_on_disk_version(&store, holder.address(), &ast),
            DRC_TRUST_LINE_LIVE_STATE_V2
        );
        assert!(!line_on_disk_authorized(&store, holder.address(), &ast));
    }

    #[test]
    fn require_auth_enable_rejected_when_liability_nonzero() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xB1);
        let holder = key(0xB2);
        let cur = std_code(b"RA2");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        fund(&store, &issuer, 500);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(
            crate::drc_issued_controls_test_harness::support::signed_issued_transfer(
                &issuer,
                holder.address(),
                &issuer,
                cur,
                5,
                1,
                0,
                &ctx,
            ),
        );
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        let nonce = issuer_drc_nonce(&store, &issuer);
        let pol = signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableRequireAuth,
            1,
            nonce,
            &ctx,
        );
        assert!(apply_policy_direct(&store, &pol, &ctx, 3).is_err());
        assert_eq!(issuer_drc_nonce(&store, &issuer), nonce);
    }

    use crate::drc_issued_controls::load_drc_issued_asset_policy;
}
