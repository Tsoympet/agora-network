//! Reusable multisign attachment adversary matrix for escrow create, finish, and cancel.

#[cfg(test)]
mod matrix {
    use agora_crypto::KeyPair;
    use agora_types::{
        materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane,
        DrcMultisignAttachmentKey, DrcMultisignBlockAttachment, DrcMultisignEntry,
        DrcMultisignOperationKind, Hash, DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
        DRC_MULTISIGN_AUTH_VERSION, DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
        DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_escrow_test_harness::multisign::{
        self, base_multisign_cancel_block, base_multisign_create_block,
        base_multisign_finish_block, install_signer_list, install_signer_list_at_nonce,
        multisign_cancel_block, multisign_finish_block, reject_preserving,
    };
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, create_live_escrow, fund, key, snapshot_escrow_state,
    };
    use crate::StateStore;
    use agora_types::{Block, NativeAssetId};

    type Tamper = fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext);

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

    fn tamper_orphan_attachment_create(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_escrow_creates.clear();
    }

    fn tamper_orphan_attachment_finish(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_escrow_finishes.clear();
    }

    fn tamper_orphan_attachment_cancel(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_escrow_cancels.clear();
    }

    fn tamper_wrong_kind_finish(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcEscrowCreate;
    }

    fn tamper_wrong_kind_cancel(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcEscrowFinish;
    }

    fn tamper_wrong_kind_create(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcTicketCreate;
    }

    fn tamper_wrong_signing_commitment_create(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }

    fn tamper_wrong_signing_commitment_finish(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }

    fn tamper_wrong_signing_commitment_cancel(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.signing_commitment = Hash([0xdd; 32]);
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
                    kind: DrcMultisignOperationKind::DrcEscrowCreate,
                    signing_commitment: Hash([0xab; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: key(1).address(),
                    signatures: vec![],
                },
            });
    }

    fn tamper_mixed_single_and_multisign_create(
        block: &mut Block,
        master: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_escrow_creates[0].public_key = master.public_key_bytes().to_vec();
        block.drc_escrow_creates[0].signature = vec![1; 64];
    }

    fn tamper_mixed_single_and_multisign_finish(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_escrow_finishes[0].public_key = submitter.public_key_bytes().to_vec();
        block.drc_escrow_finishes[0].signature = vec![1; 64];
    }

    fn tamper_mixed_single_and_multisign_cancel(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_escrow_cancels[0].public_key = submitter.public_key_bytes().to_vec();
        block.drc_escrow_cancels[0].signature = vec![1; 64];
    }

    fn tamper_tampered_signature(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
    }

    fn tamper_below_quorum_create(
        block: &mut Block,
        master: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing = block.drc_escrow_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth =
            multisign::multisign_bundle(master.address(), &signing, &[(&key(88), 1)], ctx);
    }

    fn tamper_below_quorum_finish(
        block: &mut Block,
        submitter: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing = block.drc_escrow_finishes[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth =
            multisign::multisign_bundle(submitter.address(), &signing, &[signers[0]], ctx);
        if signers.len() > 1 {
            // drop quorum by using only first signer when list requires two
            let _ = signers[1];
        }
    }

    fn tamper_foreign_signer_create(
        block: &mut Block,
        master: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing = block.drc_escrow_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
            master.address(),
            &signing,
            &[(&key(90), 1), (&key(91), 2)],
            ctx,
        );
    }

    fn tamper_foreign_signer_finish(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing = block.drc_escrow_finishes[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
            submitter.address(),
            &signing,
            &[(&key(92), 1), (&key(93), 2)],
            ctx,
        );
    }

    fn tamper_foreign_signer_cancel(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing = block.drc_escrow_cancels[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
            submitter.address(),
            &signing,
            &[(&key(94), 1), (&key(95), 2)],
            ctx,
        );
    }

    fn tamper_below_quorum_cancel(
        block: &mut Block,
        submitter: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing = block.drc_escrow_cancels[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth =
            multisign::multisign_bundle(submitter.address(), &signing, &[signers[0]], ctx);
    }

    macro_rules! create_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(10);
                let recipient = key(11);
                let s1 = key(12);
                let s2 = key(13);
                fund(&store, &master, 200_000);
                let ctx = auth();
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                let mut block = base_multisign_create_block(&store, &master, &recipient, signers, &ctx);
                let before = snapshot_escrow_state(&store, &master, &recipient);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &recipient, block, &ctx, &before);
            })*
        };
    }

    create_cases! {
        matrix_create_rejects_missing_attachment => tamper_missing_attachment,
        matrix_create_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_create_rejects_orphan_attachment => tamper_orphan_attachment_create,
        matrix_create_rejects_wrong_operation_kind => tamper_wrong_kind_create,
        matrix_create_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment_create,
        matrix_create_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_create_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_create_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_create_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_create_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_create_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign_create,
        matrix_create_rejects_tampered_signature => tamper_tampered_signature,
        matrix_create_rejects_below_quorum => tamper_below_quorum_create,
        matrix_create_rejects_foreign_signer => tamper_foreign_signer_create,
    }

    #[test]
    fn matrix_create_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let master = key(14);
        let recipient = key(15);
        let s1 = key(16);
        let s2 = key(17);
        fund(&store, &master, 200_000);
        let ctx = auth();
        let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
        let block = base_multisign_create_block(&store, &master, &recipient, signers, &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &master, &[(&key(18), 1)], &ctx, nonce);
        let before = snapshot_escrow_state(&store, &master, &recipient);
        reject_preserving(&store, &master, &recipient, block, &ctx, &before);
    }

    macro_rules! finish_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(20);
                let recipient = key(21);
                let s1 = key(22);
                let s2 = key(23);
                fund(&store, &master, 200_000);
                let ctx = auth();
                let (id, _) =
                    create_live_escrow(&store, &master, &recipient, 12, None, Some(90), 1, &ctx);
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                install_signer_list(&store, &master, signers, &ctx);
                let before = snapshot_escrow_state(&store, &master, &recipient);
                let mut block = multisign_finish_block(&store, &master, id, signers, &ctx);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &recipient, block, &ctx, &before);
            })*
        };
    }

    finish_cases! {
        matrix_finish_rejects_missing_attachment => tamper_missing_attachment,
        matrix_finish_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_finish_rejects_orphan_attachment => tamper_orphan_attachment_finish,
        matrix_finish_rejects_wrong_operation_kind => tamper_wrong_kind_finish,
        matrix_finish_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment_finish,
        matrix_finish_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_finish_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_finish_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_finish_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_finish_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_finish_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign_finish,
        matrix_finish_rejects_tampered_signature => tamper_tampered_signature,
        matrix_finish_rejects_below_quorum => tamper_below_quorum_finish,
        matrix_finish_rejects_foreign_signer => tamper_foreign_signer_finish,
    }

    #[test]
    fn matrix_finish_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let master = key(24);
        let recipient = key(25);
        let s1 = key(26);
        fund(&store, &master, 200_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 12, None, Some(90), 1, &ctx);
        let signers: &[(&KeyPair, u16)] = &[(&s1, 1)];
        install_signer_list(&store, &master, signers, &ctx);
        let block = multisign_finish_block(&store, &master, id, signers, &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &master, &[(&key(27), 1)], &ctx, nonce);
        let before = snapshot_escrow_state(&store, &master, &recipient);
        reject_preserving(&store, &master, &recipient, block, &ctx, &before);
    }

    macro_rules! cancel_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(30);
                let helper = key(31);
                let recipient = key(32);
                let s1 = key(33);
                let s2 = key(34);
                fund(&store, &master, 200_000);
                fund(&store, &helper, 500);
                let ctx = auth();
                let (id, _) =
                    create_live_escrow(&store, &master, &recipient, 12, None, Some(60), 1, &ctx);
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                install_signer_list(&store, &helper, signers, &ctx);
                let before = snapshot_escrow_state(&store, &master, &recipient);
                let mut block =
                    multisign_cancel_block(&store, &helper, &master, id, signers, &ctx);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &helper, signers, &ctx);
                reject_preserving(&store, &master, &recipient, block, &ctx, &before);
            })*
        };
    }

    cancel_cases! {
        matrix_cancel_rejects_missing_attachment => tamper_missing_attachment,
        matrix_cancel_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_cancel_rejects_orphan_attachment => tamper_orphan_attachment_cancel,
        matrix_cancel_rejects_wrong_operation_kind => tamper_wrong_kind_cancel,
        matrix_cancel_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment_cancel,
        matrix_cancel_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_cancel_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_cancel_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_cancel_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_cancel_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_cancel_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign_cancel,
        matrix_cancel_rejects_tampered_signature => tamper_tampered_signature,
        matrix_cancel_rejects_below_quorum => tamper_below_quorum_cancel,
        matrix_cancel_rejects_foreign_signer => tamper_foreign_signer_cancel,
    }

    #[test]
    fn matrix_cancel_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let master = key(35);
        let helper = key(36);
        let recipient = key(37);
        let s1 = key(38);
        fund(&store, &master, 200_000);
        fund(&store, &helper, 500);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 12, None, Some(60), 1, &ctx);
        install_signer_list(&store, &helper, &[(&s1, 1)], &ctx);
        let block = multisign_cancel_block(&store, &helper, &master, id, &[(&s1, 1)], &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &helper.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &helper, &[(&key(39), 1)], &ctx, nonce);
        let before = snapshot_escrow_state(&store, &master, &recipient);
        reject_preserving(&store, &master, &recipient, block, &ctx, &before);
    }

    #[test]
    fn matrix_create_positive_borsh_body_root_and_apply() {
        let store = StateStore::open_in_memory();
        let master = key(40);
        let recipient = key(41);
        let s1 = key(42);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let mut block = base_multisign_create_block(&store, &master, &recipient, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        apply_escrow_block(&store, decoded, 1, &ctx);
    }

    #[test]
    fn matrix_finish_positive_borsh_body_root_and_apply() {
        let store = StateStore::open_in_memory();
        let master = key(43);
        let recipient = key(44);
        let s1 = key(45);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 15, None, Some(80), 1, &ctx);
        let mut block =
            base_multisign_finish_block(&store, &master, &recipient, id, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        apply_escrow_block(&store, decoded, 2, &ctx);
    }

    #[test]
    fn matrix_cancel_positive_borsh_body_root_and_apply() {
        let store = StateStore::open_in_memory();
        let master = key(46);
        let helper = key(47);
        let recipient = key(48);
        let s1 = key(49);
        fund(&store, &master, 50_000);
        fund(&store, &helper, 500);
        let ctx = auth();
        let (id, _) = create_live_escrow(&store, &master, &recipient, 20, None, Some(40), 1, &ctx);
        let mut block = base_multisign_cancel_block(
            &store,
            &helper,
            &master,
            &recipient,
            id,
            &[(&s1, 1)],
            &ctx,
        );
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        apply_escrow_block(&store, decoded, 40, &ctx);
    }
}
