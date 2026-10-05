//! 15-mode multisign adversary grids for policy set, issuer control, and clawback.

#[cfg(test)]
mod policy_set_grid {
    use agora_crypto::KeyPair;
    use agora_types::{
        validate_drc_multisign_attachment_lane, DrcMultisignAttachmentKey,
        DrcMultisignBlockAttachment, DrcMultisignOperationKind, Hash,
        DRC_MULTISIGN_ATTACHMENT_KEY_VERSION, DRC_MULTISIGN_AUTH_VERSION,
        DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION, DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::apply::TxAuthContext;
    use crate::drc_issued_controls_test_harness::multisign::{
        base_multisign_policy_set_block, resign_policy_master,
    };
    use crate::drc_issued_controls_test_harness::support::{
        auth, controls_roots, fund, issuer_drc_nonce, key, reject_apply_preserving, std_code,
    };
    use crate::StateStore;
    use agora_types::Block;

    type Tamper = fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext);

    fn run(issuer: &KeyPair, signers: &[(&KeyPair, u16)], tamper: Tamper) {
        let store = StateStore::open_in_memory();
        fund(&store, issuer, 200);
        let ctx = auth();
        let cur = std_code(b"MP1");
        let ast = crate::drc_issued_controls_test_harness::support::asset(issuer, cur);
        let mut block = base_multisign_policy_set_block(&store, issuer, cur, signers, &ctx);
        let roots_before = controls_roots(&store);
        let nonce_before = issuer_drc_nonce(&store, issuer);
        tamper(&mut block, issuer, signers, &ctx);
        reject_apply_preserving(&store, &block, &ctx, roots_before, issuer, &ast);
        assert_eq!(issuer_drc_nonce(&store, issuer), nonce_before);
    }

    fn tamper_missing_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments.clear();
    }
    fn tamper_duplicate_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments
            .push(b.drc_multisign_attachments[0].clone());
    }
    fn tamper_orphan_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_issued_asset_policy_sets.clear();
    }
    fn tamper_wrong_kind(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcTrustLineSet;
    }
    fn tamper_wrong_signing_commitment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }
    fn tamper_wrong_signing_for(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].auth.signing_for = key(77).address();
    }
    fn tamper_noncanonical_attachment_order(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut extra = b.drc_multisign_attachments[0].clone();
        extra.key.signing_commitment = Hash([0x01; 32]);
        b.drc_multisign_attachments.insert(0, extra);
    }
    fn tamper_unsorted_signer_entries(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut entries = b.drc_multisign_attachments[0].auth.signatures.clone();
        entries.reverse();
        b.drc_multisign_attachments[0].auth.signatures = entries;
    }
    fn tamper_oversized_auth_entries(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let sample = b.drc_multisign_attachments[0].auth.signatures[0].clone();
        b.drc_multisign_attachments[0]
            .auth
            .signatures
            .resize(DRC_MULTISIGN_MAX_SIGNATURES + 1, sample);
    }
    fn tamper_oversized_attachment_lane(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments
            .push(DrcMultisignBlockAttachment {
                version: DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: DrcMultisignAttachmentKey {
                    version: DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: DrcMultisignOperationKind::DrcIssuedTransfer,
                    signing_commitment: Hash([0xab; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: key(1).address(),
                    signatures: vec![],
                },
            });
    }
    fn tamper_mixed_single_and_multisign(
        b: &mut Block,
        issuer: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        resign_policy_master(b, issuer, ctx);
    }
    fn tamper_tampered_signature(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
    }
    fn tamper_below_quorum(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].auth.signatures.pop();
    }
    fn tamper_foreign_signer(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].auth.signatures[0].signer = key(88).address();
    }

    macro_rules! grid {
        ($($name:ident => $t:expr,)*) => {
            $(#[test] fn $name() {
                let issuer = key(1);
                let s1 = key(2);
                let s2 = key(3);
                run(&issuer, &[(&s1, 1), (&s2, 2)], $t);
            })*
        };
    }

    grid! {
        matrix_policy_set_rejects_missing_attachment => tamper_missing_attachment,
        matrix_policy_set_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_policy_set_rejects_orphan_attachment => tamper_orphan_attachment,
        matrix_policy_set_rejects_wrong_operation_kind => tamper_wrong_kind,
        matrix_policy_set_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment,
        matrix_policy_set_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_policy_set_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_policy_set_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_policy_set_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_policy_set_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_policy_set_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign,
        matrix_policy_set_rejects_tampered_signature => tamper_tampered_signature,
        matrix_policy_set_rejects_below_quorum => tamper_below_quorum,
        matrix_policy_set_rejects_foreign_signer => tamper_foreign_signer,
    }

    #[test]
    fn matrix_policy_set_positive_materialize_borsh_body_root_and_apply() {
        use borsh::BorshDeserialize;

        let store = StateStore::open_in_memory();
        let issuer = key(10);
        let s1 = key(11);
        fund(&store, &issuer, 300);
        let ctx = auth();
        let cur = std_code(b"MPS");
        let block = base_multisign_policy_set_block(&store, &issuer, cur, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, decoded.compute_body_root());
        crate::drc_issued_controls_test_harness::support::apply_block(&store, decoded, 3, &ctx);
        assert!(
            crate::drc_issued_controls::load_drc_issued_asset_policy(
                &store,
                &crate::drc_issued_controls_test_harness::support::asset(&issuer, cur)
            )
            .unwrap()
            .global_freeze
        );
    }
}

#[cfg(test)]
mod issuer_control_grid {
    use agora_crypto::KeyPair;
    use agora_types::{
        DrcMultisignAttachmentKey, DrcMultisignBlockAttachment, DrcMultisignOperationKind, Hash,
        DRC_MULTISIGN_ATTACHMENT_KEY_VERSION, DRC_MULTISIGN_AUTH_VERSION,
        DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION, DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::apply::TxAuthContext;
    use crate::drc_issued_controls_test_harness::multisign::{
        base_multisign_issuer_control_block, resign_issuer_control_master,
    };
    use crate::drc_issued_controls_test_harness::support::{
        auth, controls_roots, fund, issuer_drc_nonce, key, reject_apply_preserving,
        setup_live_line, std_code,
    };
    use crate::StateStore;
    use agora_types::Block;

    type Tamper = fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext);

    fn run(issuer: &KeyPair, holder: &KeyPair, signers: &[(&KeyPair, u16)], tamper: Tamper) {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let cur = std_code(b"MI1");
        setup_live_line(&store, holder, issuer, cur, 50, 1);
        fund(&store, issuer, 200);
        let ast = crate::drc_issued_controls_test_harness::support::asset(issuer, cur);
        let mut block = base_multisign_issuer_control_block(
            &store,
            issuer,
            holder.address(),
            cur,
            signers,
            &ctx,
        );
        let roots_before = controls_roots(&store);
        let nonce_before = issuer_drc_nonce(&store, issuer);
        tamper(&mut block, issuer, signers, &ctx);
        reject_apply_preserving(&store, &block, &ctx, roots_before, issuer, &ast);
        assert_eq!(issuer_drc_nonce(&store, issuer), nonce_before);
    }

    fn tamper_missing_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments.clear();
    }
    fn tamper_orphan_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_trust_line_issuer_controls.clear();
    }
    fn tamper_wrong_kind(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].key.kind =
            DrcMultisignOperationKind::DrcIssuedAssetPolicySet;
    }
    fn tamper_mixed_single_and_multisign(
        b: &mut Block,
        issuer: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        resign_issuer_control_master(b, issuer, ctx);
    }
    fn tamper_tampered_signature(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
    }
    fn tamper_below_quorum(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].auth.signatures.pop();
    }
    fn tamper_foreign_signer(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].auth.signatures[0].signer = key(88).address();
    }
    fn tamper_duplicate_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments
            .push(b.drc_multisign_attachments[0].clone());
    }
    fn tamper_wrong_signing_commitment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }
    fn tamper_wrong_signing_for(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].auth.signing_for = key(77).address();
    }
    fn tamper_noncanonical_attachment_order(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut extra = b.drc_multisign_attachments[0].clone();
        extra.key.signing_commitment = Hash([0x01; 32]);
        b.drc_multisign_attachments.insert(0, extra);
    }
    fn tamper_unsorted_signer_entries(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut entries = b.drc_multisign_attachments[0].auth.signatures.clone();
        entries.reverse();
        b.drc_multisign_attachments[0].auth.signatures = entries;
    }
    fn tamper_oversized_auth_entries(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let sample = b.drc_multisign_attachments[0].auth.signatures[0].clone();
        b.drc_multisign_attachments[0]
            .auth
            .signatures
            .resize(DRC_MULTISIGN_MAX_SIGNATURES + 1, sample);
    }
    fn tamper_oversized_attachment_lane(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments
            .push(DrcMultisignBlockAttachment {
                version: DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: DrcMultisignAttachmentKey {
                    version: DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: DrcMultisignOperationKind::DrcIssuedClawback,
                    signing_commitment: Hash([0xab; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: key(1).address(),
                    signatures: vec![],
                },
            });
    }

    macro_rules! grid {
        ($($name:ident => $t:expr,)*) => {
            $(#[test] fn $name() {
                let issuer = key(4);
                let holder = key(5);
                let s1 = key(6);
                let s2 = key(7);
                run(&issuer, &holder, &[(&s1, 1), (&s2, 2)], $t);
            })*
        };
    }

    grid! {
        matrix_issuer_control_rejects_missing_attachment => tamper_missing_attachment,
        matrix_issuer_control_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_issuer_control_rejects_orphan_attachment => tamper_orphan_attachment,
        matrix_issuer_control_rejects_wrong_operation_kind => tamper_wrong_kind,
        matrix_issuer_control_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment,
        matrix_issuer_control_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_issuer_control_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_issuer_control_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_issuer_control_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_issuer_control_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_issuer_control_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign,
        matrix_issuer_control_rejects_tampered_signature => tamper_tampered_signature,
        matrix_issuer_control_rejects_below_quorum => tamper_below_quorum,
        matrix_issuer_control_rejects_foreign_signer => tamper_foreign_signer,
    }
}

#[cfg(test)]
mod clawback_grid {
    use agora_crypto::KeyPair;
    use agora_types::{
        DrcIssuedAssetPolicyAction, DrcMultisignAttachmentKey, DrcMultisignBlockAttachment,
        DrcMultisignOperationKind, Hash, DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
        DRC_MULTISIGN_AUTH_VERSION, DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
        DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::apply::TxAuthContext;
    use crate::drc_issued_controls_test_harness::multisign::{
        base_multisign_clawback_block, resign_clawback_master,
    };
    use crate::drc_issued_controls_test_harness::support::{
        apply_block, auth, controls_roots, fund, issuer_drc_nonce, key, reject_apply_preserving,
        signed_issued_transfer, signed_policy_set, std_code,
    };
    use crate::StateStore;
    use agora_types::Block;

    type Tamper = fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext);

    fn prepare_clawback_state(
        store: &StateStore,
        issuer: &KeyPair,
        holder: &KeyPair,
        cur: agora_types::IssuedCurrencyCode,
        ctx: &crate::apply::TxAuthContext,
    ) {
        fund(&store, issuer, 500);
        fund(&store, holder, 50);
        let mut block =
            crate::drc_trust_line_test_harness::support::coinbase(vec![Hash::ZERO], holder);
        block.drc_trust_line_sets.push(
            crate::drc_issued_controls_test_harness::support::signed_trust_line_set(
                holder, issuer, cur, 100, 1, 0, ctx,
            ),
        );
        block.header.tx_root = block.compute_body_root();
        apply_block(store, block, 1, ctx);
        crate::drc_issued_controls_test_harness::support::apply_policy_direct(
            store,
            &signed_policy_set(
                issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableClawback,
                1,
                0,
                ctx,
            ),
            ctx,
            2,
        )
        .unwrap();
        apply_block(
            store,
            {
                let mut b =
                    crate::drc_trust_line_test_harness::support::coinbase(vec![Hash::ZERO], issuer);
                b.drc_issued_transfers.push(signed_issued_transfer(
                    issuer,
                    holder.address(),
                    issuer,
                    cur,
                    20,
                    1,
                    1,
                    ctx,
                ));
                b.header.tx_root = b.compute_body_root();
                b
            },
            3,
            ctx,
        );
    }

    fn run(issuer: &KeyPair, holder: &KeyPair, signers: &[(&KeyPair, u16)], tamper: Tamper) {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let cur = std_code(b"MC1");
        prepare_clawback_state(&store, issuer, holder, cur, &ctx);
        let ast = crate::drc_issued_controls_test_harness::support::asset(issuer, cur);
        let mut block =
            base_multisign_clawback_block(&store, issuer, holder.address(), cur, signers, &ctx);
        let roots_before = controls_roots(&store);
        let nonce_before = issuer_drc_nonce(&store, issuer);
        tamper(&mut block, issuer, signers, &ctx);
        reject_apply_preserving(&store, &block, &ctx, roots_before, issuer, &ast);
        assert_eq!(issuer_drc_nonce(&store, issuer), nonce_before);
    }

    fn tamper_missing_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments.clear();
    }
    fn tamper_orphan_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_issued_clawbacks.clear();
    }
    fn tamper_wrong_kind(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].key.kind =
            DrcMultisignOperationKind::DrcTrustLineIssuerControl;
    }
    fn tamper_mixed_single_and_multisign(
        b: &mut Block,
        issuer: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        resign_clawback_master(b, issuer, ctx);
    }
    fn tamper_tampered_signature(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
    }
    fn tamper_below_quorum(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].auth.signatures.pop();
    }
    fn tamper_foreign_signer(b: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        b.drc_multisign_attachments[0].auth.signatures[0].signer = key(88).address();
    }
    fn tamper_duplicate_attachment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments
            .push(b.drc_multisign_attachments[0].clone());
    }
    fn tamper_wrong_signing_commitment(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }
    fn tamper_wrong_signing_for(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments[0].auth.signing_for = key(77).address();
    }
    fn tamper_noncanonical_attachment_order(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut extra = b.drc_multisign_attachments[0].clone();
        extra.key.signing_commitment = Hash([0x01; 32]);
        b.drc_multisign_attachments.insert(0, extra);
    }
    fn tamper_unsorted_signer_entries(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut entries = b.drc_multisign_attachments[0].auth.signatures.clone();
        entries.reverse();
        b.drc_multisign_attachments[0].auth.signatures = entries;
    }
    fn tamper_oversized_auth_entries(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let sample = b.drc_multisign_attachments[0].auth.signatures[0].clone();
        b.drc_multisign_attachments[0]
            .auth
            .signatures
            .resize(DRC_MULTISIGN_MAX_SIGNATURES + 1, sample);
    }
    fn tamper_oversized_attachment_lane(
        b: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        b.drc_multisign_attachments
            .push(DrcMultisignBlockAttachment {
                version: DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: DrcMultisignAttachmentKey {
                    version: DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: DrcMultisignOperationKind::DrcIssuedAssetPolicySet,
                    signing_commitment: Hash([0xab; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: key(1).address(),
                    signatures: vec![],
                },
            });
    }

    macro_rules! grid {
        ($($name:ident => $t:expr,)*) => {
            $(#[test] fn $name() {
                let issuer = key(8);
                let holder = key(9);
                let s1 = key(10);
                let s2 = key(11);
                run(&issuer, &holder, &[(&s1, 1), (&s2, 2)], $t);
            })*
        };
    }

    grid! {
        matrix_clawback_rejects_missing_attachment => tamper_missing_attachment,
        matrix_clawback_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_clawback_rejects_orphan_attachment => tamper_orphan_attachment,
        matrix_clawback_rejects_wrong_operation_kind => tamper_wrong_kind,
        matrix_clawback_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment,
        matrix_clawback_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_clawback_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_clawback_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_clawback_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_clawback_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_clawback_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign,
        matrix_clawback_rejects_tampered_signature => tamper_tampered_signature,
        matrix_clawback_rejects_below_quorum => tamper_below_quorum,
        matrix_clawback_rejects_foreign_signer => tamper_foreign_signer,
    }
}
