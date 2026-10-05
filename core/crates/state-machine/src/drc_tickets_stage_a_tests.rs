//! Stage-A consensus tests: ticket lane order, atomic sequence, reorg journal, multisign create.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_multisign_participant_bound, sign_drc_payment_bound, sign_drc_ticket_create_bound,
        KeyPair,
    };
    use agora_types::{
        materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane, Amount,
        Block, BlockHeader, DrcAccountSequenceSelector, DrcMultisignAuth, DrcMultisignEntry,
        DrcPaymentTx, DrcSignerListEntry, DrcSignerListTx, DrcTicketCreateTx, Hash, NativeAssetId,
        Transaction, TxOut, DRC_MULTISIGN_AUTH_VERSION, DRC_PAYMENT_LEGACY_VERSION,
        DRC_PAYMENT_TICKET_VERSION,
    };
    use borsh::BorshDeserialize;

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{
        apply_block_batched_with_auth_at_blue_score, revert_journal_batched, TxAuthContext,
    };
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::store::WriteBatch;
    use crate::supply::{
        load_burned_supply, load_issued_supply, put_burned_supply_into, put_issued_supply_into,
        put_schema_version_into,
    };
    use crate::{StateStore, SCHEMA_VERSION};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn key(b: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[b; 32]).unwrap()
    }

    fn fund(store: &StateStore, kp: &KeyPair, amount: u64) {
        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(amount),
        )
        .unwrap();
        let issued = load_issued_supply(store, NativeAssetId::DRC)
            .unwrap()
            .checked_add(amount)
            .unwrap();
        put_issued_supply_into(&mut batch, NativeAssetId::DRC, issued);
        put_burned_supply_into(
            &mut batch,
            NativeAssetId::DRC,
            load_burned_supply(store, NativeAssetId::DRC).unwrap(),
        );
        put_schema_version_into(&mut batch, SCHEMA_VERSION);
        store.write_batch(batch).unwrap();
    }

    fn coinbase_block(master: &KeyPair) -> Block {
        coinbase_block_with_nonce(master, 1)
    }

    fn coinbase_block_with_nonce(master: &KeyPair, tx_version: u32) -> Block {
        Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: tx_version as u64,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![Transaction::unsigned(
                tx_version,
                vec![],
                vec![TxOut {
                    value: Amount::from_base_units(50),
                    address: master.address(),
                }],
                1,
            )],
        )
    }

    #[test]
    fn same_block_create_then_ticket_payment_via_apply_block() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let payee = key(2);
        fund(&store, &owner, 500);
        let ctx = auth();

        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, 0);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            payee.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(1));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut block = coinbase_block(&owner);
        block.drc_ticket_creates.push(create);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();

        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            result.acceptance.drc_ticket_create_statuses,
            vec![agora_types::TransactionAcceptance::Accepted]
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            2
        );
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .is_empty());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &payee.address())
                .unwrap()
                .balance,
            10
        );
    }

    #[test]
    fn payment_rejection_preserves_ticket_state() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        fund(&store, &owner, 5);
        let ctx = auth();

        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, 0);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut create_block = coinbase_block(&owner);
        create_block.drc_ticket_creates.push(create);
        create_block.header.tx_root = create_block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &create_block, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![1]
        );

        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(4).address(),
            Amount::from_base_units(100),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(1));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut pay_block = coinbase_block_with_nonce(&owner, 2);
        pay_block.drc_payments.push(payment);
        pay_block.header.tx_root = pay_block.compute_body_root();

        assert!(apply_block_batched_with_auth_at_blue_score(
            &store,
            &pay_block,
            50,
            Some(&ctx),
            50
        )
        .is_err());
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![1]
        );
    }

    #[test]
    fn legacy_payment_rejects_ticket_selector() {
        let payment = DrcPaymentTx {
            version: DRC_PAYMENT_LEGACY_VERSION,
            from: key(1).address(),
            to: key(2).address(),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(1)),
            last_valid_blue_score: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(payment.validate_envelope_version().is_err());
    }

    #[test]
    fn journal_revert_restores_ticket_meta() {
        let store = StateStore::open_in_memory();
        let owner = key(5);
        fund(&store, &owner, 100);
        let ctx = auth();
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, 0);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase_block(&owner);
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![1]
        );
        let revert = revert_journal_batched(&result.journal).unwrap();
        store.write_batch(revert).unwrap();
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn multisign_ticket_create_survives_body_v12_borsh_roundtrip() {
        let store = StateStore::open_in_memory();
        let master = key(10);
        let signer_a = key(11);
        let signer_b = key(12);
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
        agora_crypto::sign_drc_signer_list_bound(
            &mut install,
            &master,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
        let mut block = coinbase_block(&master);
        block.drc_signer_lists.push(install);
        block.header.tx_root = block.compute_body_root();
        let r0 = apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
            .unwrap();
        store.write_batch(r0.batch).unwrap();

        let mut create = DrcTicketCreateTx::unsigned(master.address(), Amount::ZERO, 1);
        create.public_key.clear();
        create.signature.clear();
        let signing_bytes = create.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        let mut entries = Vec::new();
        for kp in [&signer_a, &signer_b] {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                master.address(),
                &signing_bytes,
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
        create.multisign = Some(DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: master.address(),
            signatures: entries,
        });

        let mut block2 = coinbase_block_with_nonce(&master, 3);
        block2.header.parents = vec![block.id()];
        block2.drc_ticket_creates.push(create);
        materialize_drc_multisign_attachments(&mut block2, &ctx.chain_id, &ctx.genesis).unwrap();
        block2.header.tx_root = block2.compute_body_root();
        let bytes = borsh::to_vec(&block2).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert!(!decoded.drc_ticket_creates.is_empty());
        validate_drc_multisign_attachment_lane(&decoded, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut apply_block = decoded;
        apply_block.header.tx_root = apply_block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &apply_block, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            load_drc_account_tickets(&store, &master.address()).unwrap(),
            vec![2]
        );
    }
}
