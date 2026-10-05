//! Nonce, Ticket, and authorization matrices for trust line operations.

#[cfg(test)]
mod ticket_retention {
    use agora_crypto::sign_drc_ticket_create_bound;
    use agora_types::{
        DrcAccountSequenceSelector, DrcTicketCreateTx, Hash, NativeAssetId,
        DRC_TRUST_LINE_SET_TICKET_VERSION,
    };

    use crate::accounts::load_account;
    use crate::drc_mempool::{lookup_drc_ticket_point, DrcTicketPointStatus};
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::drc_trust_line_test_harness::support::{
        auth, coinbase, fund, key, signed_trust_line_set, std_code,
    };
    use crate::StateStore;

    fn mint_ticket(store: &StateStore, owner: &agora_crypto::KeyPair) -> u64 {
        let ctx = auth();
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut create =
            DrcTicketCreateTx::unsigned(owner.address(), agora_types::Amount::ZERO, nonce);
        sign_drc_ticket_create_bound(&mut create, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        store
            .write_batch(
                crate::apply::apply_block_batched_with_auth_at_blue_score(
                    store,
                    &block,
                    50,
                    Some(&ctx),
                    50,
                )
                .unwrap()
                .batch,
            )
            .unwrap();
        nonce + 1
    }

    #[test]
    fn ticket_retained_on_trust_line_set_policy_style_failure() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let holder = key(1);
        let issuer = key(2);
        fund(&store, &holder, 100);
        fund(&store, &issuer, 100);
        let ticket = mint_ticket(&store, &holder);
        let mut tx = signed_trust_line_set(&holder, &issuer, std_code(b"TIX"), 10, 1, 0, &ctx);
        tx.version = DRC_TRUST_LINE_SET_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        tx.limit = agora_types::IssuedAmount::ZERO;
        agora_crypto::sign_drc_trust_line_set_bound(&mut tx, &holder, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_trust_line_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            &store,
            &block,
            50,
            Some(&ctx),
            60
        )
        .is_err());
        assert_eq!(
            lookup_drc_ticket_point(&store, &holder.address(), ticket).unwrap(),
            DrcTicketPointStatus::Live
        );
        assert!(load_drc_account_tickets(&store, &holder.address())
            .unwrap()
            .contains(&ticket));
    }
}
