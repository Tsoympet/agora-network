//! Policy, line control, clawback, liability, native barrier, same-block, and RocksDB hardening.

#[cfg(test)]
mod tests {
    use agora_types::{
        DrcIssuedAssetPolicyAction, DrcTrustLineIssuerControlAction, Hash, NativeAssetId,
        TransactionAcceptance,
    };

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_virtual_at_blue_score;
    use crate::drc_issued_controls::{
        load_drc_issued_asset_policy, load_drc_issued_asset_policy_receipt,
        load_drc_issued_clawback_receipt, load_drc_trust_line_issuer_control_receipt,
    };
    use crate::drc_issued_controls_test_harness::support::{
        apply_block, apply_block_journal, apply_issuer_control_direct, apply_policy_direct,
        assert_liability_matches_holders, asset, auth, controls_roots, fund, issuer_drc_nonce,
        issuer_outstanding, key, line_balance, revert_journal, setup_live_line, signed_clawback,
        signed_issued_transfer, signed_issuer_control, signed_policy_set, std_code,
    };
    use crate::drc_trust_line_test_harness::support::coinbase;
    use crate::StateStore;

    #[test]
    fn policy_duplicate_require_auth_rejects_without_fee_or_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let cur = std_code(b"P01");
        let ast = asset(&issuer, cur);
        fund(&store, &issuer, 100);
        let n0 = issuer_drc_nonce(&store, &issuer);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableRequireAuth,
                1,
                n0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        let n1 = issuer_drc_nonce(&store, &issuer);
        let roots = controls_roots(&store);
        assert!(apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableRequireAuth,
                1,
                n1,
                &ctx,
            ),
            &ctx,
            2,
        )
        .is_err());
        assert_eq!(issuer_drc_nonce(&store, &issuer), n1);
        assert_eq!(controls_roots(&store), roots);
        assert!(
            load_drc_issued_asset_policy(&store, &ast)
                .unwrap()
                .require_auth
        );
    }

    #[test]
    fn no_freeze_rejects_when_line_frozen() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(2);
        let holder = key(3);
        let cur = std_code(b"P02");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 200);
        let n = issuer_drc_nonce(&store, &issuer);
        apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                holder.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineFrozen(true),
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .unwrap();
        let n2 = issuer_drc_nonce(&store, &issuer);
        let roots = controls_roots(&store);
        assert!(apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableNoFreeze,
                1,
                n2,
                &ctx,
            ),
            &ctx,
            3,
        )
        .is_err());
        assert_eq!(issuer_drc_nonce(&store, &issuer), n2);
        assert_eq!(controls_roots(&store), roots);
    }

    #[test]
    fn deep_freeze_requires_line_freeze_and_clear_order() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(4);
        let holder = key(5);
        let cur = std_code(b"P03");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 200);
        let n = issuer_drc_nonce(&store, &issuer);
        assert!(apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                holder.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineDeepFrozen(true),
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .is_err());
        apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                holder.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineFrozen(true),
                1,
                n,
                &ctx,
            ),
            &ctx,
            2,
        )
        .unwrap();
        let n1 = issuer_drc_nonce(&store, &issuer);
        apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                holder.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineDeepFrozen(true),
                1,
                n1,
                &ctx,
            ),
            &ctx,
            3,
        )
        .unwrap();
        let n2 = issuer_drc_nonce(&store, &issuer);
        assert!(apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                holder.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineFrozen(false),
                1,
                n2,
                &ctx,
            ),
            &ctx,
            4,
        )
        .is_err());
    }

    #[test]
    fn clawback_exact_partial_to_zero_and_liability() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(6);
        let holder = key(7);
        let cur = std_code(b"C01");
        let ast = asset(&issuer, cur);
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
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            30,
            1,
            1,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        assert_liability_matches_holders(&store, &ast);
        let n = issuer_drc_nonce(&store, &issuer);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_clawbacks.push(signed_clawback(
            &issuer,
            holder.address(),
            cur,
            30,
            1,
            n,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 3, &ctx);
        assert_eq!(line_balance(&store, holder.address(), &ast), 0);
        assert_eq!(issuer_outstanding(&store, &ast), 0);
        assert_liability_matches_holders(&store, &ast);
    }

    #[test]
    fn native_drc_balances_change_only_by_fees_in_control_block() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(8);
        let holder = key(9);
        let cur = std_code(b"N01");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 1_000);
        let addrs = [issuer.address(), holder.address()];
        let sum = |s: &StateStore| -> u64 {
            addrs
                .iter()
                .map(|a| load_account(s, NativeAssetId::DRC, a).unwrap().balance)
                .sum()
        };
        let before = sum(&store);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_asset_policy_sets.push(signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            2,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 2, &ctx);
        assert_eq!(sum(&store), before - 2);
    }

    #[test]
    fn same_block_policy_then_control_then_issue_then_clawback() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x10);
        let holder = key(0x11);
        let cur = std_code(b"SB1");
        let ast = asset(&issuer, cur);
        setup_live_line(&store, &holder, &issuer, cur, 200, 1);
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
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block
            .drc_trust_line_issuer_controls
            .push(signed_issuer_control(
                &issuer,
                holder.address(),
                cur,
                DrcTrustLineIssuerControlAction::AuthorizeHolder,
                1,
                2,
                &ctx,
            ));
        block.drc_issued_transfers.push(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur,
            10,
            1,
            3,
            &ctx,
        ));
        block.drc_issued_clawbacks.push(signed_clawback(
            &issuer,
            holder.address(),
            cur,
            4,
            1,
            4,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 10, &ctx);
        assert_eq!(line_balance(&store, holder.address(), &ast), 6);
        assert_eq!(issuer_outstanding(&store, &ast), 6);
        assert_liability_matches_holders(&store, &ast);
    }

    #[test]
    fn issue_before_authorize_same_block_conflict_lost() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x20);
        let holder = key(0x21);
        let cur = std_code(b"SB2");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        fund(&store, &issuer, 500);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableRequireAuth,
                1,
                0,
                &ctx,
            ),
            &ctx,
            1,
        )
        .unwrap();
        let roots = controls_roots(&store);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
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
            apply_block_batched_virtual_at_blue_score(&store, &block, 50, Some(&ctx), 11).unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            result.acceptance.drc_issued_transfer_statuses,
            vec![TransactionAcceptance::ConflictLost]
        );
        assert_eq!(controls_roots(&store), roots);
    }

    #[test]
    fn journal_revert_restores_policy_and_line_flags() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x30);
        let holder = key(0x31);
        let cur = std_code(b"JR1");
        let ast = asset(&issuer, cur);
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        fund(&store, &issuer, 200);
        let pol = signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            1,
            0,
            &ctx,
        );
        let policy_id = pol.policy_set_tx_id();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_asset_policy_sets.push(pol);
        block.header.tx_root = block.compute_body_root();
        let journal = apply_block_journal(&store, block, 5, &ctx);
        assert!(
            load_drc_issued_asset_policy(&store, &ast)
                .unwrap()
                .global_freeze
        );
        assert!(load_drc_issued_asset_policy_receipt(&store, &policy_id)
            .unwrap()
            .is_some());
        revert_journal(&store, &journal);
        assert!(
            !load_drc_issued_asset_policy(&store, &ast)
                .unwrap()
                .global_freeze
        );
        assert!(load_drc_issued_asset_policy_receipt(&store, &policy_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn journal_revert_removes_control_and_clawback_receipts() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x32);
        let holder = key(0x33);
        let cur = std_code(b"JR2");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
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
            2,
        )
        .unwrap();
        let mut issue_block = coinbase(vec![Hash::ZERO], &issuer);
        issue_block
            .drc_issued_transfers
            .push(signed_issued_transfer(
                &issuer,
                holder.address(),
                &issuer,
                cur,
                10,
                1,
                1,
                &ctx,
            ));
        issue_block.header.tx_root = issue_block.compute_body_root();
        apply_block(&store, issue_block, 3, &ctx);

        let control = signed_issuer_control(
            &issuer,
            holder.address(),
            cur,
            DrcTrustLineIssuerControlAction::SetLineFrozen(true),
            1,
            2,
            &ctx,
        );
        let clawback = signed_clawback(&issuer, holder.address(), cur, 4, 1, 3, &ctx);
        let control_id = control.issuer_control_tx_id();
        let clawback_id = clawback.clawback_tx_id();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_trust_line_issuer_controls.push(control);
        block.drc_issued_clawbacks.push(clawback);
        block.header.tx_root = block.compute_body_root();
        let journal = apply_block_journal(&store, block, 4, &ctx);

        assert!(
            load_drc_trust_line_issuer_control_receipt(&store, &control_id)
                .unwrap()
                .is_some()
        );
        assert!(load_drc_issued_clawback_receipt(&store, &clawback_id)
            .unwrap()
            .is_some());
        revert_journal(&store, &journal);
        assert!(
            load_drc_trust_line_issuer_control_receipt(&store, &control_id)
                .unwrap()
                .is_none()
        );
        assert!(load_drc_issued_clawback_receipt(&store, &clawback_id)
            .unwrap()
            .is_none());
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_reopen_preserves_policy_and_v2_line_flags() {
        use crate::drc_issued_controls_test_harness::support::signed_trust_line_set;
        use crate::state_root::compose_trident_state_root;
        use agora_types::DRC_TRUST_LINE_LIVE_STATE_V2;

        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path()).unwrap();
        let ctx = auth();
        let issuer = key(0x40);
        let holder = key(0x41);
        let cur = std_code(b"RK1");
        let ast = asset(&issuer, cur);
        fund(&store, &issuer, 500);
        fund(&store, &holder, 50);
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            &holder, &issuer, cur, 100, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 1, &ctx);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableRequireAuth,
                1,
                0,
                &ctx,
            ),
            &ctx,
            2,
        )
        .unwrap();
        // Direct policy mutation bypasses block acceptance. The composed root
        // refuses a stale common object mirror, so rebuild it before commit.
        crate::reindex_drc_ledger_objects(&store).unwrap();
        let tip = Hash([7; 32]);
        let root = compose_trident_state_root(&store, &tip).unwrap();
        drop(store);
        let reopened = StateStore::open(dir.path()).unwrap();
        assert!(
            load_drc_issued_asset_policy(&reopened, &ast)
                .unwrap()
                .require_auth
        );
        assert_eq!(
            crate::drc_issued_controls_test_harness::support::line_on_disk_version(
                &reopened,
                holder.address(),
                &ast
            ),
            DRC_TRUST_LINE_LIVE_STATE_V2
        );
        assert_eq!(compose_trident_state_root(&reopened, &tip).unwrap(), root);
    }

    #[test]
    fn borsh_block_roundtrip_preserves_body_root_for_control_lanes() {
        use borsh::BorshDeserialize;
        let ctx = auth();
        let issuer = key(0x50);
        let cur = std_code(b"BR1");
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_asset_policy_sets.push(signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            1,
            0,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = agora_types::Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.compute_body_root(), block.compute_body_root());
    }
}
