//! Multisign attachment adversary matrix for trust line set (15-mode grid).

#[cfg(test)]
mod trust_line_set_matrix {
    use agora_crypto::KeyPair;
    use agora_types::{
        validate_drc_multisign_attachment_lane, DrcMultisignAttachmentKey,
        DrcMultisignBlockAttachment, DrcMultisignOperationKind, Hash,
        DRC_MULTISIGN_ATTACHMENT_KEY_VERSION, DRC_MULTISIGN_AUTH_VERSION,
        DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION, DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_trust_line::count_live_trust_lines_for_holder;
    use crate::drc_trust_line_test_harness::multisign::{
        base_multisign_trust_line_set_block, install_signer_list, install_signer_list_at_nonce,
    };
    use crate::drc_trust_line_test_harness::support::{auth, fund, key, trust_root};
    use crate::StateStore;
    use agora_types::{Block, NativeAssetId};

    type Tamper = fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext);

    fn reject_preserving(
        store: &StateStore,
        holder: &KeyPair,
        block: Block,
        ctx: &TxAuthContext,
        root_before: agora_types::Hash,
        count_before: usize,
    ) {
        if validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).is_err() {
            assert_eq!(trust_root(store), root_before);
            assert_eq!(
                count_live_trust_lines_for_holder(store, &holder.address()).unwrap(),
                count_before
            );
            return;
        }
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            store,
            &block,
            50,
            Some(ctx),
            50
        )
        .is_err());
        assert_eq!(trust_root(store), root_before);
        assert_eq!(
            count_live_trust_lines_for_holder(store, &holder.address()).unwrap(),
            count_before
        );
    }

    fn tamper_missing_attachment(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments.clear();
    }

    fn tamper_duplicate_attachment(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block
            .drc_multisign_attachments
            .push(block.drc_multisign_attachments[0].clone());
    }

    fn tamper_orphan_attachment(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_trust_line_sets.clear();
    }

    fn tamper_wrong_kind(block: &mut Block, _: &KeyPair, _: &[(&KeyPair, u16)], _: &TxAuthContext) {
        block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcIssuedTransfer;
    }

    fn tamper_wrong_signing_commitment(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }

    fn tamper_wrong_signing_for(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].auth.signing_for = key(77).address();
    }

    fn tamper_noncanonical_attachment_order(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut extra = block.drc_multisign_attachments[0].clone();
        extra.key.signing_commitment = Hash([0x01; 32]);
        block.drc_multisign_attachments.insert(0, extra);
    }

    fn tamper_unsorted_signer_entries(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let mut entries = block.drc_multisign_attachments[0].auth.signatures.clone();
        entries.reverse();
        block.drc_multisign_attachments[0].auth.signatures = entries;
    }

    fn tamper_oversized_auth_entries(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        let sample = block.drc_multisign_attachments[0].auth.signatures[0].clone();
        block.drc_multisign_attachments[0]
            .auth
            .signatures
            .resize(DRC_MULTISIGN_MAX_SIGNATURES + 1, sample);
    }

    fn tamper_oversized_attachment_lane(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block
            .drc_multisign_attachments
            .push(DrcMultisignBlockAttachment {
                version: DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: DrcMultisignAttachmentKey {
                    version: DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: DrcMultisignOperationKind::DrcTrustLineSet,
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
        block: &mut Block,
        master: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        agora_crypto::sign_drc_trust_line_set_bound(
            &mut block.drc_trust_line_sets[0],
            master,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
    }

    fn tamper_tampered_signature(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
    }

    fn tamper_below_quorum(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].auth.signatures.pop();
    }

    fn tamper_foreign_signer(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].auth.signatures[0].signer = key(88).address();
    }

    macro_rules! matrix_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let holder = key(1);
                let issuer = key(2);
                let s1 = key(3);
                let s2 = key(4);
                fund(&store, &holder, 100);
                fund(&store, &issuer, 100);
                let ctx = auth();
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                let root_before = trust_root(&store);
                let count_before =
                    count_live_trust_lines_for_holder(&store, &holder.address()).unwrap();
                let mut block =
                    base_multisign_trust_line_set_block(&store, &holder, &issuer, signers, &ctx);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &holder, signers, &ctx);
                reject_preserving(&store, &holder, block, &ctx, root_before, count_before);
            })*
        };
    }

    matrix_cases! {
        matrix_trust_line_set_rejects_missing_attachment => tamper_missing_attachment,
        matrix_trust_line_set_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_trust_line_set_rejects_orphan_attachment => tamper_orphan_attachment,
        matrix_trust_line_set_rejects_wrong_operation_kind => tamper_wrong_kind,
        matrix_trust_line_set_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment,
        matrix_trust_line_set_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_trust_line_set_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_trust_line_set_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_trust_line_set_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_trust_line_set_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_trust_line_set_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign,
        matrix_trust_line_set_rejects_tampered_signature => tamper_tampered_signature,
        matrix_trust_line_set_rejects_below_quorum => tamper_below_quorum,
        matrix_trust_line_set_rejects_foreign_signer => tamper_foreign_signer,
    }

    #[test]
    fn matrix_trust_line_set_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let holder = key(10);
        let issuer = key(11);
        let s1 = key(12);
        fund(&store, &holder, 100);
        fund(&store, &issuer, 100);
        let ctx = auth();
        install_signer_list(&store, &holder, &[(&s1, 1)], &ctx);
        let block =
            base_multisign_trust_line_set_block(&store, &holder, &issuer, &[(&s1, 1)], &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &holder.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &holder, &[(&key(13), 1)], &ctx, nonce);
        let root_before = trust_root(&store);
        reject_preserving(
            &store,
            &holder,
            block,
            &ctx,
            root_before,
            count_live_trust_lines_for_holder(&store, &holder.address()).unwrap(),
        );
    }
}
