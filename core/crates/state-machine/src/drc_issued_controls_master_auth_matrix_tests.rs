//! Master disable, regular key, and signer-list recovery for policy, control, clawback.

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_drc_issued_clawback_bound, sign_drc_regular_key_bound, KeyPair};
    use agora_types::{
        Amount, DrcIssuedAssetPolicyAction, DrcRegularKeyTx, DrcTrustLineIssuerControlAction, Hash,
        NativeAssetId,
    };

    use crate::accounts::load_account;
    use crate::drc_issued_controls_test_harness::multisign::{
        base_multisign_clawback_block, base_multisign_issuer_control_block,
        base_multisign_policy_set_block,
    };
    use crate::drc_issued_controls_test_harness::support::{
        apply_block, apply_clawback_direct, apply_policy_direct, auth, coinbase, fund, key,
        reject_apply_preserving, setup_live_line, signed_clawback, signed_policy_set, std_code,
    };
    use crate::drc_policy::apply_drc_account_policy;
    use crate::drc_regular_key::apply_drc_regular_key;
    use crate::drc_trust_line_test_harness::multisign::{install_signer_list, multisign_bundle};
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn disable_master(store: &StateStore, owner: &KeyPair, ctx: &crate::apply::TxAuthContext) {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut tx = agora_types::DrcAccountPolicyTx::set_master_key_disabled(
            owner.address(),
            Amount::ZERO,
            nonce,
        );
        agora_crypto::sign_drc_account_policy_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn install_regular(
        store: &StateStore,
        master: &KeyPair,
        regular: &KeyPair,
        ctx: &crate::apply::TxAuthContext,
    ) {
        let nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut tx = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            nonce,
        );
        sign_drc_regular_key_bound(&mut tx, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn matrix_policy_set_disabled_master_rejected_multisign_recovers() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x51);
        let regular = key(0x52);
        let s1 = key(0x53);
        fund(&store, &issuer, 200);
        install_regular(&store, &issuer, &regular, &ctx);
        install_signer_list(&store, &issuer, &[(&s1, 1)], &ctx);
        disable_master(&store, &issuer, &ctx);
        let cur = std_code(b"IA1");
        let ast = crate::drc_issued_controls_test_harness::support::asset(&issuer, cur);
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_asset_policy_sets.push(signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            1,
            load_account(&store, NativeAssetId::DRC, &issuer.address())
                .unwrap()
                .nonce,
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        reject_apply_preserving(
            &store,
            &block,
            &ctx,
            crate::drc_issued_controls_test_harness::support::controls_roots(&store),
            &issuer,
            &ast,
        );
        let nonce = load_account(&store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = agora_types::DrcIssuedAssetPolicySetTx {
            version: agora_types::DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
            issuer: issuer.address(),
            currency: cur,
            action: DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            issuer.address(),
            &signing_bytes,
            &[(&s1, 1)],
            &ctx,
        ));
        let mut ok = coinbase(vec![Hash::ZERO], &issuer);
        ok.drc_issued_asset_policy_sets.push(tx);
        agora_types::materialize_drc_multisign_attachments(&mut ok, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        ok.header.tx_root = ok.compute_body_root();
        apply_block(&store, ok, 1, &ctx);
    }

    #[test]
    fn matrix_issuer_control_multisign_semantic_failure_preserves_state() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x61);
        let holder = key(0x62);
        let s1 = key(0x63);
        let cur = std_code(b"IA2");
        fund(&store, &issuer, 200);
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        let ast = crate::drc_issued_controls_test_harness::support::asset(&issuer, cur);
        let mut block = base_multisign_issuer_control_block(
            &store,
            &issuer,
            holder.address(),
            cur,
            &[(&s1, 1)],
            &ctx,
        );
        block.drc_trust_line_issuer_controls[0].action =
            DrcTrustLineIssuerControlAction::SetLineDeepFrozen(true);
        block.header.tx_root = block.compute_body_root();
        reject_apply_preserving(
            &store,
            &block,
            &ctx,
            crate::drc_issued_controls_test_harness::support::controls_roots(&store),
            &issuer,
            &ast,
        );
    }

    #[test]
    fn matrix_clawback_regular_key_after_master_disabled() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x71);
        let regular = key(0x72);
        let holder = key(0x73);
        let cur = std_code(b"IA3");
        fund(&store, &issuer, 300);
        fund(&store, &holder, 50);
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
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
        crate::drc_issued_controls_test_harness::support::apply_block(
            &store,
            {
                let mut b = coinbase(vec![Hash::ZERO], &issuer);
                b.drc_issued_transfers.push(
                    crate::drc_issued_controls_test_harness::support::signed_issued_transfer(
                        &issuer,
                        holder.address(),
                        &issuer,
                        cur,
                        20,
                        3,
                        1,
                        &ctx,
                    ),
                );
                b.header.tx_root = b.compute_body_root();
                b
            },
            4,
            &ctx,
        );
        install_regular(&store, &issuer, &regular, &ctx);
        disable_master(&store, &issuer, &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = signed_clawback(&issuer, holder.address(), cur, 5, 1, nonce, &ctx);
        assert!(apply_clawback_direct(&store, &tx, &ctx, 5).is_err());
        sign_drc_issued_clawback_bound(&mut tx, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        apply_clawback_direct(&store, &tx, &ctx, 6).unwrap();
    }

    #[test]
    fn matrix_policy_set_multisign_grid_base_applies() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x81);
        let s1 = key(0x82);
        fund(&store, &issuer, 100);
        let cur = std_code(b"IA4");
        let block = base_multisign_policy_set_block(&store, &issuer, cur, &[(&s1, 1)], &ctx);
        apply_block(&store, block, 1, &ctx);
        assert!(
            crate::load_drc_issued_asset_policy(
                &store,
                &crate::drc_issued_controls_test_harness::support::asset(&issuer, cur)
            )
            .unwrap()
            .global_freeze
        );
    }

    #[test]
    fn matrix_clawback_multisign_base_requires_balance() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0x91);
        let holder = key(0x92);
        let s1 = key(0x93);
        let cur = std_code(b"IA5");
        fund(&store, &issuer, 200);
        setup_live_line(&store, &holder, &issuer, cur, 10, 1);
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
        let ast = crate::drc_issued_controls_test_harness::support::asset(&issuer, cur);
        let block = base_multisign_clawback_block(
            &store,
            &issuer,
            holder.address(),
            cur,
            &[(&s1, 1)],
            &ctx,
        );
        reject_apply_preserving(
            &store,
            &block,
            &ctx,
            crate::drc_issued_controls_test_harness::support::controls_roots(&store),
            &issuer,
            &ast,
        );
    }
}
