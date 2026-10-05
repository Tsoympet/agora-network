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

    #[test]
    fn matrix_trust_line_set_positive_materialize_borsh_body_root_and_apply() {
        use agora_types::validate_drc_multisign_attachment_lane;
        use borsh::BorshDeserialize;

        let store = StateStore::open_in_memory();
        let holder = key(20);
        let issuer = key(21);
        let s1 = key(22);
        fund(&store, &holder, 100);
        fund(&store, &issuer, 100);
        let ctx = auth();
        let mut block =
            base_multisign_trust_line_set_block(&store, &holder, &issuer, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        crate::drc_trust_line_test_harness::support::apply_block(&store, decoded, 1, &ctx);
        assert_eq!(
            count_live_trust_lines_for_holder(&store, &holder.address()).unwrap(),
            1
        );
    }
}

#[cfg(test)]
mod issued_transfer_matrix {
    use agora_crypto::KeyPair;
    use agora_types::{
        validate_drc_multisign_attachment_lane, DrcMultisignAttachmentKey,
        DrcMultisignBlockAttachment, DrcMultisignOperationKind, Hash,
        DRC_MULTISIGN_ATTACHMENT_KEY_VERSION, DRC_MULTISIGN_AUTH_VERSION,
        DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION, DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::apply::TxAuthContext;
    use crate::drc_trust_line_test_harness::invariants;
    use crate::drc_trust_line_test_harness::multisign::{
        base_multisign_issued_transfer_block, install_signer_list, install_signer_list_at_nonce,
    };
    use crate::drc_trust_line_test_harness::support::{auth, fund, key, setup_live_line, std_code};
    use crate::StateStore;
    use agora_types::Block;

    type Tamper = fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext);

    fn run_tamper(name: &str, tamper: Tamper) {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let holder = key(2);
        let recipient = key(3);
        let s1 = key(4);
        let s2 = key(5);
        let cur = std_code(b"IOU");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        setup_live_line(&store, &recipient, &issuer, cur, 100, 2);
        fund(&store, &holder, 200);
        fund(&store, &issuer, 200);
        let mut ib =
            crate::drc_trust_line_test_harness::support::coinbase(vec![Hash::ZERO], &issuer);
        ib.drc_issued_transfers.push(
            crate::drc_trust_line_test_harness::support::signed_issued_transfer(
                &issuer,
                holder.address(),
                &issuer,
                cur,
                20,
                1,
                0,
                &ctx,
            ),
        );
        ib.header.tx_root = ib.compute_body_root();
        crate::drc_trust_line_test_harness::support::apply_block(&store, ib, 3, &ctx);
        let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
        let mut block = base_multisign_issued_transfer_block(
            &store,
            &holder,
            recipient.address(),
            &issuer,
            cur,
            signers,
            &ctx,
        );
        tamper(&mut block, &holder, signers, &ctx);
        invariants::reject_block_preserving(
            &store,
            &block,
            &ctx,
            &issuer,
            cur,
            &[&holder, &recipient],
            &[&holder],
        );
        let _ = name;
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
        b.drc_issued_transfers.clear();
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
        master: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        agora_crypto::sign_drc_issued_transfer_bound(
            &mut b.drc_issued_transfers[0],
            master,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
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

    macro_rules! issued_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                run_tamper(stringify!($name), $tamper);
            })*
        };
    }

    issued_cases! {
        matrix_issued_transfer_rejects_missing_attachment => tamper_missing_attachment,
        matrix_issued_transfer_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_issued_transfer_rejects_orphan_attachment => tamper_orphan_attachment,
        matrix_issued_transfer_rejects_wrong_operation_kind => tamper_wrong_kind,
        matrix_issued_transfer_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment,
        matrix_issued_transfer_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_issued_transfer_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_issued_transfer_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_issued_transfer_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_issued_transfer_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_issued_transfer_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign,
        matrix_issued_transfer_rejects_tampered_signature => tamper_tampered_signature,
        matrix_issued_transfer_rejects_below_quorum => tamper_below_quorum,
        matrix_issued_transfer_rejects_foreign_signer => tamper_foreign_signer,
    }

    #[test]
    fn matrix_issued_transfer_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(10);
        let holder = key(11);
        let recipient = key(12);
        let s1 = key(13);
        let cur = std_code(b"STK");
        setup_live_line(&store, &holder, &issuer, cur, 50, 1);
        setup_live_line(&store, &recipient, &issuer, cur, 50, 2);
        fund(&store, &holder, 100);
        install_signer_list(&store, &holder, &[(&s1, 1)], &ctx);
        let block = base_multisign_issued_transfer_block(
            &store,
            &holder,
            recipient.address(),
            &issuer,
            cur,
            &[(&s1, 1)],
            &ctx,
        );
        let nonce = crate::accounts::load_account(
            &store,
            agora_types::NativeAssetId::DRC,
            &holder.address(),
        )
        .unwrap()
        .nonce;
        install_signer_list_at_nonce(&store, &holder, &[(&key(14), 1)], &ctx, nonce);
        invariants::reject_block_preserving(
            &store,
            &block,
            &ctx,
            &issuer,
            cur,
            &[&holder, &recipient],
            &[&holder],
        );
    }

    #[test]
    fn matrix_issued_transfer_positive_materialize_borsh_body_root_and_apply() {
        use borsh::BorshDeserialize;

        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(30);
        let holder = key(31);
        let recipient = key(32);
        let s1 = key(33);
        let cur = std_code(b"POS");
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        setup_live_line(&store, &recipient, &issuer, cur, 100, 2);
        fund(&store, &holder, 500);
        fund(&store, &issuer, 500);
        let mut ib =
            crate::drc_trust_line_test_harness::support::coinbase(vec![Hash::ZERO], &issuer);
        ib.drc_issued_transfers.push(
            crate::drc_trust_line_test_harness::support::signed_issued_transfer(
                &issuer,
                holder.address(),
                &issuer,
                cur,
                30,
                1,
                0,
                &ctx,
            ),
        );
        ib.header.tx_root = ib.compute_body_root();
        crate::drc_trust_line_test_harness::support::apply_block(&store, ib, 3, &ctx);
        let mut block = base_multisign_issued_transfer_block(
            &store,
            &holder,
            recipient.address(),
            &issuer,
            cur,
            &[(&s1, 1)],
            &ctx,
        );
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        crate::drc_trust_line_test_harness::support::apply_block(&store, decoded, 3, &ctx);
    }
}
