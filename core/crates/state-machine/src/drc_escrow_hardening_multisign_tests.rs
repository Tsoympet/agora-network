//! Multisign materialization and adversarial attachment cases for escrow ops.

#[cfg(test)]
mod escrow_multisign {
    use agora_crypto::{sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair};
    use agora_types::{
        materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane, Amount,
        DrcEscrowCreateTx, DrcMultisignAuth, DrcMultisignEntry, DrcMultisignOperationKind,
        DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId, DRC_ESCROW_CREATE_TX_VERSION,
        DRC_MULTISIGN_AUTH_VERSION,
    };

    use crate::accounts::load_account;
    use crate::apply::apply_block_batched_with_auth_at_blue_score;
    use crate::drc_escrow_test_harness::support::{
        apply_escrow_block, auth, coinbase, create_live_escrow, fund, key, signed_cancel,
        signed_create,
    };
    use crate::drc_signer_list::apply_drc_signer_list;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn install_signer_list(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &crate::apply::TxAuthContext,
    ) {
        let total: u16 = signers.iter().map(|(_, w)| w).sum();
        let mut install = DrcSignerListTx::unsigned_set(
            master.address(),
            agora_types::canonical_sorted_entries(
                &signers
                    .iter()
                    .map(|(kp, weight)| DrcSignerListEntry {
                        signer: kp.address(),
                        weight: *weight,
                    })
                    .collect::<Vec<_>>(),
            ),
            u32::from(total),
            Amount::ZERO,
            0,
        );
        sign_drc_signer_list_bound(&mut install, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(store, &install, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    fn multisign_bundle(
        owner: agora_types::Address,
        signing_bytes: &[u8],
        signers: &[(&KeyPair, u16)],
        ctx: &crate::apply::TxAuthContext,
    ) -> DrcMultisignAuth {
        let mut entries = Vec::new();
        for (kp, _) in signers {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                owner,
                signing_bytes,
                kp,
                &ctx.chain_id,
                &ctx.genesis,
            )
            .unwrap();
            entries.push(DrcMultisignEntry {
                signer,
                public_key,
                signature,
            });
        }
        entries.sort_by_key(|e| e.signer.0);
        DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: owner,
            signatures: entries,
        }
    }

    #[test]
    fn create_multisign_materialize_validate_apply() {
        let store = StateStore::open_in_memory();
        let master = key(60);
        let s1 = key(61);
        let s2 = key(62);
        let recipient = key(63);
        fund(&store, &master, 50_000);
        let ctx = auth();
        install_signer_list(&store, &master, &[(&s1, 1), (&s2, 1)], &ctx);
        let signers = [(&s1, 1), (&s2, 1)];

        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: master.address(),
            recipient: recipient.address(),
            amount: Amount::from_base_units(40),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: None,
            cancel_after_blue_score: Some(100),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        create.public_key.clear();
        create.signature.clear();
        create.multisign = Some(multisign_bundle(
            master.address(),
            &create.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &signers,
            &ctx,
        ));

        let mut block = coinbase(vec![Hash::ZERO], &master);
        block.drc_escrow_creates.push(create);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 1).unwrap();
        store.write_batch(result.batch).unwrap();
    }

    #[test]
    fn cancel_multisign_submitter_not_owner() {
        let store = StateStore::open_in_memory();
        let master = key(64);
        let helper = key(65);
        let s1 = key(66);
        let recipient = key(67);
        fund(&store, &master, 20_000);
        fund(&store, &helper, 500);
        let ctx = auth();
        install_signer_list(&store, &helper, &[(&s1, 1)], &ctx);
        let (id, _) = create_live_escrow(&store, &master, &recipient, 25, None, Some(40), 1, &ctx);
        let mut cancel = signed_cancel(
            &helper,
            id,
            load_account(&store, NativeAssetId::DRC, &helper.address())
                .unwrap()
                .nonce,
            &ctx,
        );
        cancel.public_key.clear();
        cancel.signature.clear();
        cancel.multisign = Some(multisign_bundle(
            helper.address(),
            &cancel.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&s1, 1)],
            &ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], &master);
        block.drc_escrow_cancels.push(cancel);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        apply_escrow_block(&store, block, 40, &ctx);
    }

    #[test]
    fn orphan_attachment_rejected_for_escrow_create() {
        let ctx = auth();
        let owner = key(70);
        let create = signed_create(&owner, key(71).address(), 5, None, Some(20), 0, &ctx, None);
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_escrow_creates.push(create);
        block
            .drc_multisign_attachments
            .push(agora_types::DrcMultisignBlockAttachment {
                version: agora_types::DRC_MULTISIGN_BLOCK_ATTACHMENT_VERSION,
                key: agora_types::DrcMultisignAttachmentKey {
                    version: agora_types::DRC_MULTISIGN_ATTACHMENT_KEY_VERSION,
                    kind: DrcMultisignOperationKind::DrcEscrowCreate,
                    signing_commitment: Hash([9; 32]),
                },
                auth: DrcMultisignAuth {
                    version: DRC_MULTISIGN_AUTH_VERSION,
                    signing_for: owner.address(),
                    signatures: vec![],
                },
            });
        assert!(
            validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).is_err()
        );
    }
}
