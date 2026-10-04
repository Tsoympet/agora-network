//! Block-lane tests: template → Borsh → merge → apply for DRC multisign attachments.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_multisign_participant_bound, sign_drc_signer_list_bound,
        KeyPair,
    };
    use agora_types::{
        materialize_drc_multisign_attachments, merge_drc_multisign_attachments,
        validate_drc_multisign_attachment_lane, Amount, Block, BlockHeader, DrcMultisignAuth,
        DrcMultisignEntry, DrcMultisignOperationKind, DrcPaymentTx,
        DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId, Transaction,
        DRC_MULTISIGN_AUTH_VERSION,
    };
    use borsh::BorshDeserialize;

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{apply_block_batched_with_auth, TxAuthContext};
    use crate::drc_signer_list::apply_drc_signer_list;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn key(byte: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[byte; 32]).unwrap()
    }

    fn fund(store: &StateStore, account: &KeyPair, amount: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &account.address(),
            Amount::from_base_units(amount),
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn multisign_bundle(
        owner: agora_types::Address,
        operation_signing_bytes: &[u8],
        signers: &[(&KeyPair, u16)],
        auth: &TxAuthContext,
    ) -> DrcMultisignAuth {
        let mut entries: Vec<DrcMultisignEntry> = Vec::new();
        for (kp, _) in signers {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                owner,
                operation_signing_bytes,
                kp,
                &auth.chain_id,
                &auth.genesis,
            )
            .unwrap();
            entries.push(DrcMultisignEntry {
                signer,
                public_key,
                signature,
            });
        }
        entries.sort_by_key(|entry| entry.signer.0);
        DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: owner,
            signatures: entries,
        }
    }

    fn header_for(block: &Block) -> BlockHeader {
        let mut header = block.header.clone();
        header.tx_root = block.compute_body_root();
        header
    }

    #[test]
    fn payment_multisign_survives_materialize_borsh_merge_apply() {
        let store = StateStore::open_in_memory();
        let master = key(1);
        let signer_a = key(2);
        let signer_b = key(3);
        let recipient = key(4);
        fund(&store, &master, 500);
        let ctx = auth();

        let mut install = DrcSignerListTx::unsigned_set(
            master.address(),
            agora_types::canonical_sorted_entries(&[
                DrcSignerListEntry {
                    signer: signer_a.address(),
                    weight: 1,
                },
                DrcSignerListEntry {
                    signer: signer_b.address(),
                    weight: 2,
                },
            ]),
            2,
            Amount::from_base_units(1),
            0,
        );
        sign_drc_signer_list_bound(&mut install, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(&store, &install, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            recipient.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            1,
            Hash::ZERO,
            1,
        );
        payment.public_key.clear();
        payment.signature.clear();
        payment.multisign = Some(multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&signer_a, 1), (&signer_b, 2)],
            &ctx,
        ));

        use agora_types::{Amount as Amt, TxOut};

        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![Transaction::unsigned(
                1,
                vec![],
                vec![TxOut {
                    value: Amt::from_base_units(50),
                    address: master.address(),
                }],
                1,
            )],
        );
        block.drc_payments.push(payment);

        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(block.drc_payments[0].multisign.is_none());
        assert_eq!(block.drc_multisign_attachments.len(), 1);
        block.header = header_for(&block);

        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.drc_multisign_attachments.len(), 1);
        assert!(decoded.drc_payments[0].multisign.is_none());
        assert_eq!(decoded.header.tx_root, decoded.compute_body_root());

        validate_drc_multisign_attachment_lane(&decoded, &ctx.chain_id, &ctx.genesis).unwrap();
        let merged =
            merge_drc_multisign_attachments(decoded.clone(), &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(merged.drc_payments[0].multisign.is_some());

        let mut apply_block = decoded;
        apply_block.header.tx_root = apply_block.compute_body_root();
        let result = apply_block_batched_with_auth(&store, &apply_block, 50, Some(&ctx)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            10
        );
    }

    #[test]
    fn attachment_body_root_v11_binds_auth_bytes() {
        let ctx = auth();
        let master = key(1);
        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            key(2).address(),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            1,
        );
        payment.public_key.clear();
        payment.signature.clear();
        let bundle = multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&key(3), 1)],
            &ctx,
        );
        payment.multisign = Some(bundle.clone());

        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_payments.push(payment);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        let root = block.compute_body_root();
        block.drc_multisign_attachments[0].auth.signatures[0].signature[0] ^= 0xff;
        assert_ne!(block.compute_body_root(), root);
    }

    #[test]
    fn reject_missing_duplicate_and_orphan_attachments() {
        let ctx = auth();
        let master = key(1);
        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            key(2).address(),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            1,
        );
        payment.public_key.clear();
        payment.signature.clear();

        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_payments.push(payment.clone());
        assert!(
            validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).is_err()
        );

        payment.multisign = Some(multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&key(3), 1)],
            &ctx,
        ));
        block.drc_payments[0] = payment;
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut dup = block.clone();
        dup.drc_multisign_attachments
            .push(dup.drc_multisign_attachments[0].clone());
        assert!(validate_drc_multisign_attachment_lane(&dup, &ctx.chain_id, &ctx.genesis).is_err());

        let mut orphan = block.clone();
        orphan.drc_payments.clear();
        assert!(
            validate_drc_multisign_attachment_lane(&orphan, &ctx.chain_id, &ctx.genesis).is_err()
        );
    }

    #[test]
    fn legacy_v10_body_root_unchanged_without_attachments() {
        let block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        let root = block.compute_body_root();
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.compute_body_root(), root);
    }

    #[test]
    fn wrong_kind_attachment_key_rejected() {
        let ctx = auth();
        let master = key(1);
        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            key(2).address(),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            1,
        );
        payment.public_key.clear();
        payment.signature.clear();
        payment.multisign = Some(multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&key(3), 1)],
            &ctx,
        ));
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_payments.push(payment);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.drc_multisign_attachments[0].key.kind = DrcMultisignOperationKind::DrcStake;
        assert!(
            validate_drc_multisign_attachment_lane(&block, &ctx.chain_id, &ctx.genesis).is_err()
        );
    }

    #[test]
    fn full_block_borsh_roundtrip_preserves_attachments() {
        let ctx = auth();
        let master = key(1);
        let mut payment = DrcPaymentTx::unsigned(
            master.address(),
            key(2).address(),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            0,
            Hash::ZERO,
            1,
        );
        payment.public_key.clear();
        payment.signature.clear();
        payment.multisign = Some(multisign_bundle(
            master.address(),
            &payment.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&key(3), 1)],
            &ctx,
        ));
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_payments.push(payment);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();

        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, block);
    }
}
