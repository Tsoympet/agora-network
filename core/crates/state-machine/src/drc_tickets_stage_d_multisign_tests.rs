//! Stage-D: multisign attachment adversaries for ticket create and ticket spend.

#[cfg(test)]
mod multisign_matrix {
    use agora_crypto::{sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair};
    use agora_types::{
        materialize_drc_multisign_attachments, merge_drc_multisign_attachments,
        validate_drc_multisign_attachment_lane, Amount, Block, DrcMultisignAuth, DrcMultisignEntry,
        DrcMultisignOperationKind, DrcPaymentTx, DrcSignerListEntry, DrcSignerListTx,
        DrcTicketCreateTx, Hash, NativeAssetId, DRC_MULTISIGN_AUTH_VERSION,
        DRC_PAYMENT_TICKET_VERSION,
    };

    use crate::accounts::load_account;
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_signer_list::apply_drc_signer_list;
    use crate::drc_tickets_test_harness::support::{
        assert_ticket_snapshot_unchanged, auth, coinbase, fund, key, mint_ticket,
        reject_invalid_multisign_or_apply_preserving_ticket_state, snapshot_ticket_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn install_signer_list(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
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
        ctx: &TxAuthContext,
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

    fn reject_apply_preserving_tickets(
        store: &StateStore,
        owner: &KeyPair,
        mut block: Block,
        ctx: &TxAuthContext,
    ) {
        let before = snapshot_ticket_state(store, owner);
        block.header.tx_root = block.compute_body_root();
        reject_invalid_multisign_or_apply_preserving_ticket_state(
            store, owner, &block, ctx, &before,
        );
    }

    fn base_ticket_create_block(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, master, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = DrcTicketCreateTx::unsigned(master.address(), Amount::ZERO, nonce);
        create.public_key.clear();
        create.signature.clear();
        create.multisign = Some(multisign_bundle(
            master.address(),
            &create.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], master);
        block.drc_ticket_creates.push(create);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    fn base_ticket_payment_block(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> (Block, u64) {
        install_signer_list(store, master, signers, ctx);
        let seq = mint_ticket(store, master, ctx, vec![Hash::ZERO]).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            master.address(),
            key(99).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(agora_types::DrcAccountSequenceSelector::ticket(seq));
        payment.public_key.clear();
        payment.signature.clear();
        payment.multisign = Some(multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], master);
        block.drc_payments.push(payment);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        (block, seq)
    }

    macro_rules! ticket_create_multisign_case {
        ($name:ident, $tamper:expr) => {
            #[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(40);
                let signer_a = key(41);
                let signer_b = key(42);
                fund(&store, &master, 500_000);
                let ctx = auth();
                let signers: &[(&KeyPair, u16)] = &[(&signer_a, 1), (&signer_b, 2)];
                let mut block = base_ticket_create_block(&store, &master, signers, &ctx);
                let tamper: fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext) = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_apply_preserving_tickets(&store, &master, block, &ctx);
            }
        };
    }

    macro_rules! ticket_payment_multisign_case {
        ($name:ident, $tamper:expr) => {
            #[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let master = key(50);
                let signer_a = key(51);
                let signer_b = key(52);
                fund(&store, &master, 500_000);
                let ctx = auth();
                let signers: &[(&KeyPair, u16)] = &[(&signer_a, 1), (&signer_b, 2)];
                let (mut block, _seq) = base_ticket_payment_block(&store, &master, signers, &ctx);
                let tamper: fn(&mut Block, &KeyPair, &[(&KeyPair, u16)], &TxAuthContext) = $tamper;
                tamper(&mut block, &master, signers, &ctx);
                reject_apply_preserving_tickets(&store, &master, block, &ctx);
            }
        };
    }

    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_missing_attachment,
        |block, _, _, _| block.drc_multisign_attachments.clear()
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_duplicate_attachment,
        |block, _, _, _| {
            block
                .drc_multisign_attachments
                .push(block.drc_multisign_attachments[0].clone());
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_orphan_attachment,
        |block, _, _, _| block.drc_ticket_creates.clear()
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_wrong_kind,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcPayment;
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_wrong_signing_commitment,
        |block, _, _, _| {
            block.drc_ticket_creates[0].fee = Amount::from_base_units(1);
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_wrong_signing_for_owner,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].auth.signing_for = key(77).address();
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_unsorted_entries,
        |block, _, _, _| {
            let mut entries = block.drc_multisign_attachments[0].auth.signatures.clone();
            entries.reverse();
            block.drc_multisign_attachments[0].auth.signatures = entries;
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_oversized_attachment,
        |block, master, _, ctx| {
            let signing =
                block.drc_ticket_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            let mut entries = block.drc_multisign_attachments[0].auth.signatures.clone();
            for i in 0..33 {
                let kp = KeyPair::from_secret_bytes(&[(100 + i) as u8; 32]).unwrap();
                let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                    master.address(),
                    &signing,
                    &kp,
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
            block.drc_multisign_attachments[0].auth.signatures = entries;
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_mixed_single_and_multisign,
        |block, master, _, _| {
            block.drc_ticket_creates[0].public_key = master.public_key_bytes().to_vec();
            block.drc_ticket_creates[0].signature = vec![1; 64];
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_tampered_signature,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_below_quorum,
        |block, master, _, ctx| {
            let signing =
                block.drc_ticket_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth =
                multisign_bundle(master.address(), &signing, &[(&key(41), 1)], ctx);
        }
    );
    ticket_create_multisign_case!(
        ticket_create_multisign_rejects_foreign_signer,
        |block, master, _, ctx| {
            let signing =
                block.drc_ticket_creates[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth = multisign_bundle(
                master.address(),
                &signing,
                &[(&key(88), 1), (&key(89), 2)],
                ctx,
            );
        }
    );

    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_missing_attachment,
        |block, _, _, _| block.drc_multisign_attachments.clear()
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_duplicate_attachment,
        |block, _, _, _| {
            block
                .drc_multisign_attachments
                .push(block.drc_multisign_attachments[0].clone());
        }
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_orphan_attachment,
        |block, _, _, _| block.drc_payments.clear()
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_wrong_kind,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].key.kind =
                DrcMultisignOperationKind::DrcTicketCreate;
        }
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_tampered_signature,
        |block, _, _, _| {
            block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
        }
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_below_quorum,
        |block, master, _, ctx| {
            let signing = block.drc_payments[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth =
                multisign_bundle(master.address(), &signing, &[(&key(51), 1)], ctx);
        }
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_foreign_signer,
        |block, master, _, ctx| {
            let signing = block.drc_payments[0].signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
            block.drc_multisign_attachments[0].auth = multisign_bundle(
                master.address(),
                &signing,
                &[(&key(88), 1), (&key(89), 2)],
                ctx,
            );
        }
    );
    ticket_payment_multisign_case!(
        ticket_payment_multisign_rejects_mixed_single_and_multisign,
        |block, master, _, _| {
            block.drc_payments[0].public_key = master.public_key_bytes().to_vec();
            block.drc_payments[0].signature = vec![1; 64];
        }
    );

    #[test]
    fn ticket_create_multisign_lane_validation_before_merge() {
        let store = StateStore::open_in_memory();
        let master = key(60);
        let signer_a = key(61);
        fund(&store, &master, 100_000);
        let ctx = auth();
        let mut block = base_ticket_create_block(&store, &master, &[(&signer_a, 1)], &ctx);
        block.drc_multisign_attachments.clear();
        assert!(
            validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).is_err()
        );
        assert!(merge_drc_multisign_attachments(block, &ctx.chain_id, &ctx.genesis).is_err());
    }
}
