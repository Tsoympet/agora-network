//! Master disable, regular key, and multisign recovery matrices for both ops.

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_drc_regular_key_bound, KeyPair};
    use agora_types::{DrcRegularKeyTx, Hash, NativeAssetId};

    use crate::accounts::load_account;
    use crate::drc_policy::apply_drc_account_policy;
    use crate::drc_regular_key::apply_drc_regular_key;
    use crate::drc_trust_line_test_harness::invariants;
    use crate::drc_trust_line_test_harness::multisign::base_multisign_issued_transfer_block;
    use crate::drc_trust_line_test_harness::support::{
        apply_block, auth, coinbase, fund, key, setup_live_line, signed_trust_line_set, std_code,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn disable_master(store: &StateStore, owner: &KeyPair, ctx: &crate::apply::TxAuthContext) {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut tx = agora_types::DrcAccountPolicyTx::set_master_key_disabled(
            owner.address(),
            agora_types::Amount::ZERO,
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
            agora_types::Amount::ZERO,
            nonce,
        );
        sign_drc_regular_key_bound(&mut tx, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn matrix_trust_line_set_disabled_master_rejected_regular_recovers() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(1);
        let regular = key(2);
        let issuer = key(3);
        fund(&store, &holder, 100);
        fund(&store, &issuer, 100);
        install_regular(&store, &holder, &regular, &ctx);
        let s1 = key(4);
        crate::drc_trust_line_test_harness::multisign::install_signer_list(
            &store,
            &holder,
            &[(&s1, 1)],
            &ctx,
        );
        disable_master(&store, &holder, &ctx);
        let cur = std_code(b"MK1");
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block
            .drc_trust_line_sets
            .push(signed_trust_line_set(&holder, &issuer, cur, 10, 1, 0, &ctx));
        block.header.tx_root = block.compute_body_root();
        invariants::reject_block_preserving(&store, &block, &ctx, &issuer, cur, &[&holder], &[]);
        let nonce = load_account(&store, NativeAssetId::DRC, &holder.address())
            .unwrap()
            .nonce;
        let mut tx = agora_types::DrcTrustLineSetTx {
            version: agora_types::DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency: cur,
            limit: agora_types::IssuedAmount::from_units(10),
            fee: agora_types::Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(
            crate::drc_trust_line_test_harness::multisign::multisign_bundle(
                holder.address(),
                &signing_bytes,
                &[(&s1, 1)],
                &ctx,
            ),
        );
        let mut ok = coinbase(vec![Hash::ZERO], &holder);
        ok.drc_trust_line_sets.push(tx);
        agora_types::materialize_drc_multisign_attachments(&mut ok, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        ok.header.tx_root = ok.compute_body_root();
        apply_block(&store, ok, 1, &ctx);
    }

    #[test]
    fn matrix_issued_transfer_multisign_semantic_failure_still_rejected_with_recovery_keys() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(10);
        let holder = key(11);
        let recipient = key(12);
        let s1 = key(13);
        let cur = std_code(b"MK2");
        setup_live_line(&store, &holder, &issuer, cur, 5, 1);
        setup_live_line(&store, &recipient, &issuer, cur, 5, 2);
        fund(&store, &holder, 100);
        let mut block = base_multisign_issued_transfer_block(
            &store,
            &holder,
            recipient.address(),
            &issuer,
            cur,
            &[(&s1, 1)],
            &ctx,
        );
        block.drc_issued_transfers[0].amount = agora_types::IssuedAmount::from_units(999);
        block.header.tx_root = block.compute_body_root();
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
}
