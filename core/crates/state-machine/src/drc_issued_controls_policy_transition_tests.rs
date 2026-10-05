//! Asset policy transition matrix: freeze, no_freeze, clawback, idempotent rejects.

#[cfg(test)]
mod tests {
    use agora_types::DrcIssuedAssetPolicyAction;

    use crate::drc_issued_controls::load_drc_issued_asset_policy;
    use crate::drc_issued_controls_test_harness::support::{
        apply_policy_direct, asset, auth, fund, issuer_drc_nonce, key, setup_live_line,
        signed_policy_set, std_code,
    };
    use crate::StateStore;

    #[test]
    fn matrix_enable_no_freeze_rejects_while_global_freeze_active() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xC1);
        let cur = std_code(b"PF1");
        fund(&store, &issuer, 100);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
                1,
                0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        let n = issuer_drc_nonce(&store, &issuer);
        assert!(apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableNoFreeze,
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .is_err());
        assert_eq!(issuer_drc_nonce(&store, &issuer), n);
    }

    #[test]
    fn matrix_global_freeze_set_then_clear() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xC1);
        let cur = std_code(b"PF1");
        let ast = asset(&issuer, cur);
        fund(&store, &issuer, 100);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
                1,
                0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        let n = issuer_drc_nonce(&store, &issuer);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::ClearGlobalFreeze,
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .unwrap();
        assert!(
            !load_drc_issued_asset_policy(&store, &ast)
                .unwrap()
                .global_freeze
        );
    }

    #[test]
    fn matrix_no_freeze_irreversible_and_blocks_global_freeze() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xC2);
        let cur = std_code(b"PF2");
        fund(&store, &issuer, 100);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableNoFreeze,
                1,
                0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        let n = issuer_drc_nonce(&store, &issuer);
        assert!(apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .is_err());
    }

    #[test]
    fn matrix_clawback_enable_rejected_under_no_freeze_without_mutation() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xC3);
        let cur = std_code(b"PF3");
        fund(&store, &issuer, 100);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableNoFreeze,
                1,
                0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        let n = issuer_drc_nonce(&store, &issuer);
        assert!(apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableClawback,
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .is_err());
        assert_eq!(issuer_drc_nonce(&store, &issuer), n);
    }

    #[test]
    fn matrix_clawback_rejected_when_unauthorized_after_require_auth() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xC4);
        let holder = key(0xC5);
        let cur = std_code(b"PF4");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        fund(&store, &issuer, 500);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableClawback,
                1,
                0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableRequireAuth,
                1,
                1,
                &ctx,
            ),
            &ctx,
            2,
        )
        .unwrap();
        let n = issuer_drc_nonce(&store, &issuer);
        assert!(
            crate::drc_issued_controls_test_harness::support::apply_clawback_direct(
                &store,
                &crate::drc_issued_controls_test_harness::support::signed_clawback(
                    &issuer,
                    holder.address(),
                    cur,
                    1,
                    1,
                    n,
                    &ctx,
                ),
                &ctx,
                3,
            )
            .is_err()
        );
        assert_eq!(issuer_drc_nonce(&store, &issuer), n);
    }
}
