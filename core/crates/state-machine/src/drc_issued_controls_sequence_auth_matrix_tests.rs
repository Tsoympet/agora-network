//! Nonce/Ticket matrices for issued-controls operations.

#[cfg(test)]
mod tests {
    use agora_crypto::sign_drc_issued_asset_policy_set_bound;
    use agora_types::{
        DrcAccountSequenceSelector, DrcIssuedAssetPolicyAction, Hash, NativeAssetId,
        DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION,
    };

    use crate::accounts::load_account;
    use crate::drc_issued_controls_test_harness::support::{
        apply_block, apply_policy_direct, auth, coinbase, fund, issuer_drc_nonce, key, mint_ticket,
        setup_live_line, signed_issued_transfer, signed_policy_set, std_code,
    };
    use crate::drc_mempool::{lookup_drc_ticket_point, DrcTicketPointStatus};
    use crate::StateStore;

    #[test]
    fn matrix_policy_set_bad_nonce_rejects_without_fee_or_policy_mutation() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xD1);
        let cur = std_code(b"NA1");
        fund(&store, &issuer, 50);
        let nonce = issuer_drc_nonce(&store, &issuer);
        let mut tx = signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            1,
            nonce + 1,
            &ctx,
        );
        sign_drc_issued_asset_policy_set_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        assert!(apply_policy_direct(&store, &tx, &ctx, 1).is_err());
        assert_eq!(issuer_drc_nonce(&store, &issuer), nonce);
    }

    #[test]
    fn matrix_policy_set_ticket_semantic_failure_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xD2);
        let holder = key(0xD2 + 1);
        let cur = std_code(b"NA2");
        fund(&store, &issuer, 50);
        fund(&store, &holder, 50);
        setup_live_line(&store, &holder, &issuer, cur, 100, 1);
        apply_block(
            &store,
            {
                let mut b = coinbase(vec![Hash::ZERO], &issuer);
                b.drc_issued_transfers.push(signed_issued_transfer(
                    &issuer,
                    holder.address(),
                    &issuer,
                    cur,
                    5,
                    1,
                    0,
                    &ctx,
                ));
                b.header.tx_root = b.compute_body_root();
                b
            },
            2,
            &ctx,
        );
        let ticket = mint_ticket(&store, &issuer);
        let mut tx = signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableRequireAuth,
            1,
            0,
            &ctx,
        );
        tx.version = DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_issued_asset_policy_set_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_asset_policy_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        assert!(
            crate::apply::apply_block_batched_virtual_at_blue_score(
                &store,
                &block,
                50,
                Some(&ctx),
                2
            )
            .unwrap()
            .acceptance
            .drc_issued_asset_policy_set_statuses[0]
                != agora_types::TransactionAcceptance::Accepted
        );
        assert_eq!(
            lookup_drc_ticket_point(&store, &issuer.address(), ticket).unwrap(),
            DrcTicketPointStatus::Live
        );
    }

    #[test]
    fn matrix_policy_set_ticket_success_consumes_ticket_not_ordinary_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(0xD3);
        let cur = std_code(b"NA3");
        fund(&store, &issuer, 50);
        let ticket = mint_ticket(&store, &issuer);
        let nonce_before = load_account(&store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = signed_policy_set(
            &issuer,
            cur,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            1,
            0,
            &ctx,
        );
        tx.version = DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION;
        tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket));
        sign_drc_issued_asset_policy_set_bound(&mut tx, &issuer, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &issuer);
        block.drc_issued_asset_policy_sets.push(tx);
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 3, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &issuer.address())
                .unwrap()
                .nonce,
            nonce_before
        );
        assert_eq!(
            lookup_drc_ticket_point(&store, &issuer.address(), ticket).unwrap(),
            DrcTicketPointStatus::Unknown
        );
    }
}
