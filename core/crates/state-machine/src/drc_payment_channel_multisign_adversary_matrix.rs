//! Reusable multisign attachment adversary matrix for payment channel create, fund, claim, and close.

#[cfg(test)]
mod matrix {
    use agora_crypto::KeyPair;
    use agora_types::{
        validate_drc_multisign_attachment_lane, DrcMultisignAttachmentKey,
        DrcMultisignBlockAttachment, DrcMultisignOperationKind, Hash,
        DRC_MULTISIGN_ATTACHMENT_KEY_VERSION, DRC_MULTISIGN_AUTH_VERSION,
        DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION, DRC_MULTISIGN_MAX_SIGNATURES,
    };

    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_payment_channel_test_harness::multisign::{
        self, base_multisign_close_block, base_multisign_create_block, base_multisign_fund_block,
        install_signer_list, install_signer_list_at_nonce, multisign_claim_block,
        multisign_close_block, multisign_fund_block, reject_preserving,
    };
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, create_live_channel, fund, key, snapshot_channel_state,
    };
    use crate::StateStore;
    use agora_types::{Block, DrcPaymentChannelCloseKind, NativeAssetId};

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
        block.drc_payment_channel_creates.clear();
    }

    fn tamper_orphan_attachment_fund(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_payment_channel_funds.clear();
    }

    fn tamper_orphan_attachment_close(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_payment_channel_closes.clear();
    }

    fn tamper_wrong_kind_fund(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.kind =
            DrcMultisignOperationKind::DrcPaymentChannelCreate;
    }

    fn tamper_wrong_kind_close(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.kind =
            DrcMultisignOperationKind::DrcPaymentChannelClaim;
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

    fn tamper_wrong_signing_commitment_fund(
        block: &mut Block,
        _: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_multisign_attachments[0].key.signing_commitment = Hash([0xee; 32]);
    }

    fn tamper_wrong_signing_commitment_close(
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
                    kind: DrcMultisignOperationKind::DrcCheckCreate,
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
        block.drc_payment_channel_creates[0].public_key = master.public_key_bytes().to_vec();
        block.drc_payment_channel_creates[0].signature = vec![1; 64];
    }

    fn tamper_mixed_single_and_multisign_fund(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_payment_channel_funds[0].public_key = submitter.public_key_bytes().to_vec();
        block.drc_payment_channel_funds[0].signature = vec![1; 64];
    }

    fn tamper_mixed_single_and_multisign_close(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        _: &TxAuthContext,
    ) {
        block.drc_payment_channel_closes[0].public_key = submitter.public_key_bytes().to_vec();
        block.drc_payment_channel_closes[0].signature = vec![1; 64];
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
        let signing =
            block.drc_payment_channel_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth =
            multisign::multisign_bundle(master.address(), &signing, &[(&key(88), 1)], ctx);
    }

    fn tamper_below_quorum_fund(
        block: &mut Block,
        submitter: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing =
            block.drc_payment_channel_funds[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
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
        let signing =
            block.drc_payment_channel_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
            master.address(),
            &signing,
            &[(&key(90), 1), (&key(91), 2)],
            ctx,
        );
    }

    fn tamper_foreign_signer_fund(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing =
            block.drc_payment_channel_funds[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
            submitter.address(),
            &signing,
            &[(&key(92), 1), (&key(93), 2)],
            ctx,
        );
    }

    fn tamper_foreign_signer_close(
        block: &mut Block,
        submitter: &KeyPair,
        _: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing =
            block.drc_payment_channel_closes[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth = multisign::multisign_bundle(
            submitter.address(),
            &signing,
            &[(&key(94), 1), (&key(95), 2)],
            ctx,
        );
    }

    fn tamper_below_quorum_close(
        block: &mut Block,
        submitter: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let signing =
            block.drc_payment_channel_closes[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        block.drc_multisign_attachments[0].auth =
            multisign::multisign_bundle(submitter.address(), &signing, &[signers[0]], ctx);
    }

    macro_rules! create_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(10);
                let destination = key(11);
                let s1 = key(12);
                let s2 = key(13);
                fund(&store, &master, 200_000);
                let ctx = auth();
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                let mut block = base_multisign_create_block(&store, &master, &key(99), &destination, signers, &ctx);
                let before = snapshot_channel_state(&store, &master, &destination);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &destination, &block, &ctx, &before, 50);
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
        let destination = key(15);
        let s1 = key(16);
        let s2 = key(17);
        fund(&store, &master, 200_000);
        let ctx = auth();
        let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
        let block =
            base_multisign_create_block(&store, &master, &key(99), &destination, signers, &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &master, &[(&key(18), 1)], &ctx, nonce);
        let before = snapshot_channel_state(&store, &master, &destination);
        reject_preserving(&store, &master, &destination, &block, &ctx, &before, 50);
    }

    macro_rules! fund_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(20);
                let destination = key(21);
                let s1 = key(22);
                let s2 = key(23);
                fund(&store, &master, 200_000);
                fund(&store, &destination, 500);
                let ctx = auth();
                let claim_k = key(64);
                let (id, _) =
                    create_live_channel(&store, &master, &claim_k, &destination, 12, 1, &ctx);
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                let mut block = multisign_fund_block(&store, &master, id, signers, &ctx);
                let before = snapshot_channel_state(&store, &master, &destination);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &destination, &block, &ctx, &before, 50);
            })*
        };
    }

    fund_cases! {
        matrix_fund_rejects_missing_attachment => tamper_missing_attachment,
        matrix_fund_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_fund_rejects_orphan_attachment => tamper_orphan_attachment_fund,
        matrix_fund_rejects_wrong_operation_kind => tamper_wrong_kind_fund,
        matrix_fund_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment_fund,
        matrix_fund_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_fund_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_fund_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_fund_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_fund_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_fund_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign_fund,
        matrix_fund_rejects_tampered_signature => tamper_tampered_signature,
        matrix_fund_rejects_below_quorum => tamper_below_quorum_fund,
        matrix_fund_rejects_foreign_signer => tamper_foreign_signer_fund,
    }

    #[test]
    fn matrix_fund_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let master = key(24);
        let destination = key(25);
        let s1 = key(26);
        fund(&store, &master, 200_000);
        let ctx = auth();
        fund(&store, &destination, 500);
        let (id, _) = create_live_channel(&store, &master, &key(60), &destination, 12, 1, &ctx);
        let signers: &[(&KeyPair, u16)] = &[(&s1, 1)];
        install_signer_list(&store, &master, signers, &ctx);
        let block = multisign_fund_block(&store, &master, id, signers, &ctx);
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &master, &[(&key(27), 1)], &ctx, nonce);
        let before = snapshot_channel_state(&store, &master, &destination);
        reject_preserving(&store, &master, &destination, &block, &ctx, &before, 50);
    }

    macro_rules! close_cases {
        ($($name:ident => $tamper:expr,)*) => {
            $(#[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(30);
                let helper = key(31);
                let destination = key(32);
                let s1 = key(33);
                let s2 = key(34);
                fund(&store, &master, 200_000);
                fund(&store, &helper, 500);
                let ctx = auth();
                let (id, _) =
                    create_live_channel(&store, &master, &key(61), &destination, 12, 1, &ctx);
                let signers: &[(&KeyPair, u16)] = &[(&s1, 1), (&s2, 2)];
                let mut block =
                    multisign_close_block(&store, &master, id, agora_types::DrcPaymentChannelCloseKind::OwnerScheduleClose, signers, &ctx);
                let before = snapshot_channel_state(&store, &master, &destination);
                let tamper: Tamper = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_preserving(&store, &master, &destination, &block, &ctx, &before, 50);
            })*
        };
    }

    close_cases! {
        matrix_close_rejects_missing_attachment => tamper_missing_attachment,
        matrix_close_rejects_duplicate_attachment => tamper_duplicate_attachment,
        matrix_close_rejects_orphan_attachment => tamper_orphan_attachment_close,
        matrix_close_rejects_wrong_operation_kind => tamper_wrong_kind_close,
        matrix_close_rejects_wrong_signing_commitment => tamper_wrong_signing_commitment_close,
        matrix_close_rejects_wrong_signing_for => tamper_wrong_signing_for,
        matrix_close_rejects_noncanonical_attachment_order => tamper_noncanonical_attachment_order,
        matrix_close_rejects_unsorted_signer_entries => tamper_unsorted_signer_entries,
        matrix_close_rejects_oversized_auth_entries => tamper_oversized_auth_entries,
        matrix_close_rejects_oversized_attachment_lane => tamper_oversized_attachment_lane,
        matrix_close_rejects_mixed_single_and_multisign => tamper_mixed_single_and_multisign_close,
        matrix_close_rejects_tampered_signature => tamper_tampered_signature,
        matrix_close_rejects_below_quorum => tamper_below_quorum_close,
        matrix_close_rejects_foreign_signer => tamper_foreign_signer_close,
    }

    #[test]
    fn matrix_close_rejects_stale_signer_list() {
        let store = StateStore::open_in_memory();
        let master = key(35);
        let helper = key(36);
        let destination = key(37);
        let s1 = key(38);
        fund(&store, &master, 200_000);
        fund(&store, &helper, 500);
        let ctx = auth();
        let (id, _) = create_live_channel(&store, &master, &key(61), &destination, 12, 0, &ctx);
        install_signer_list(&store, &master, &[(&s1, 1)], &ctx);
        let block = multisign_close_block(
            &store,
            &master,
            id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            &[(&s1, 1)],
            &ctx,
        );
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(&store, &master, &[(&key(39), 1)], &ctx, nonce);
        let before = snapshot_channel_state(&store, &master, &destination);
        reject_preserving(&store, &master, &destination, &block, &ctx, &before, 50);
    }

    #[test]
    fn matrix_create_positive_borsh_body_root_and_apply() {
        let store = StateStore::open_in_memory();
        let master = key(40);
        let destination = key(41);
        let s1 = key(42);
        fund(&store, &master, 50_000);
        let ctx = auth();
        let mut block =
            base_multisign_create_block(&store, &master, &key(99), &destination, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        apply_channel_block(&store, decoded, 1, &ctx);
    }

    #[test]
    fn matrix_fund_positive_borsh_body_root_and_apply() {
        let store = StateStore::open_in_memory();
        let master = key(43);
        let destination = key(44);
        let s1 = key(45);
        fund(&store, &master, 50_000);
        let ctx = auth();
        fund(&store, &destination, 500);
        let (id, _) = create_live_channel(&store, &master, &key(62), &destination, 15, 1, &ctx);
        let mut block = base_multisign_fund_block(&store, &master, id, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        apply_channel_block(&store, decoded, 2, &ctx);
    }

    #[test]
    fn matrix_close_positive_borsh_body_root_and_apply() {
        let store = StateStore::open_in_memory();
        let master = key(46);
        let helper = key(47);
        let destination = key(48);
        let s1 = key(49);
        fund(&store, &master, 50_000);
        fund(&store, &helper, 500);
        let ctx = auth();
        let (id, _) = create_live_channel(&store, &master, &key(63), &destination, 20, 1, &ctx);
        let mut block = base_multisign_close_block(&store, &master, id, &[(&s1, 1)], &ctx);
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.header.tx_root = root;
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded: Block = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, root);
        apply_channel_block(&store, decoded, 40, &ctx);
    }
}
