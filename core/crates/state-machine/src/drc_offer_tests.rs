//! Consensus matrix for the native DRC order book.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_account_policy_bound, sign_drc_offer_cancel_bound, sign_drc_offer_create_bound,
        sign_drc_regular_key_bound, KeyPair,
    };
    use agora_types::{
        materialize_drc_multisign_attachments, Amount, DrcAccountPolicyTx,
        DrcAccountSequenceSelector, DrcBookAsset, DrcIssuedAssetPolicyAction, DrcOfferBook,
        DrcOfferCancelOutcome, DrcOfferCancelTx, DrcOfferCreateTx, DrcOfferCursor,
        DrcOfferFillMode, DrcOfferTimeInForce, DrcRegularKeyTx, DrcTrustLineIssuerControlAction,
        Hash, IssuedAssetId, NativeAssetId, TransactionAcceptance, DRC_OFFER_CANCEL_TICKET_VERSION,
        DRC_OFFER_CANCEL_TX_VERSION, DRC_OFFER_CREATE_TICKET_VERSION, DRC_OFFER_CREATE_TX_VERSION,
    };

    use crate::accounts::load_account;
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_issued_controls_test_harness::support::{
        apply_issuer_control_direct, apply_policy_direct, signed_clawback, signed_issuer_control,
        signed_policy_set,
    };
    use crate::drc_offer::{
        list_account_offers, list_book_offers, load_drc_offer_cancel_receipt, load_drc_offer_live,
        load_issued_offer_reserve,
    };
    use crate::drc_trust_line_test_harness::multisign::{install_signer_list, multisign_bundle};
    use crate::drc_trust_line_test_harness::support::{
        apply_block, asset, auth, coinbase, fund, issuer_outstanding, key, line_balance,
        mint_ticket, setup_live_line, signed_issued_transfer, std_code,
    };
    use crate::state_root::compose_trident_state_root;
    use crate::supply::load_burned_supply;
    use crate::{BlockAcceptanceRecord, StateError, StateStore};

    const BUY: u8 = DrcOfferFillMode::Buy as u8;
    const SELL: u8 = DrcOfferFillMode::Sell as u8;
    const GTC: u8 = DrcOfferTimeInForce::GoodTillCancel as u8;
    const IOC: u8 = DrcOfferTimeInForce::ImmediateOrCancel as u8;
    const FOK: u8 = DrcOfferTimeInForce::FillOrKill as u8;

    fn nonce(store: &StateStore, kp: &KeyPair) -> u64 {
        load_account(store, NativeAssetId::DRC, &kp.address())
            .unwrap()
            .nonce
    }

    fn balance(store: &StateStore, kp: &KeyPair) -> u64 {
        load_account(store, NativeAssetId::DRC, &kp.address())
            .unwrap()
            .balance
    }

    fn burned(store: &StateStore) -> u64 {
        load_burned_supply(store, NativeAssetId::DRC).unwrap()
    }

    fn native() -> DrcBookAsset {
        DrcBookAsset::NativeDrc
    }

    fn issued(asset_id: IssuedAssetId) -> DrcBookAsset {
        DrcBookAsset::Issued(asset_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn signed_create(
        owner: &KeyPair,
        signer: &KeyPair,
        pays: DrcBookAsset,
        pays_amt: u64,
        gets: DrcBookAsset,
        gets_amt: u64,
        fill: u8,
        tif: u8,
        expires: Option<u64>,
        nonce_value: u64,
        sequence: Option<DrcAccountSequenceSelector>,
        ctx: &TxAuthContext,
    ) -> DrcOfferCreateTx {
        let version = if sequence.is_some_and(|s| s.kind == agora_types::DrcAccountSequence::Ticket)
        {
            DRC_OFFER_CREATE_TICKET_VERSION
        } else {
            DRC_OFFER_CREATE_TX_VERSION
        };
        let mut tx = DrcOfferCreateTx {
            version,
            owner: owner.address(),
            taker_pays: pays,
            taker_pays_amount: pays_amt,
            taker_gets: gets,
            taker_gets_amount: gets_amt,
            fill_mode: fill,
            time_in_force: tif,
            fee: Amount::from_base_units(1),
            expires_after_blue_score: expires,
            nonce: nonce_value,
            account_sequence: sequence,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_offer_create_bound(&mut tx, signer, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    fn signed_cancel(
        submitter: &KeyPair,
        signer: &KeyPair,
        offer_id: Hash,
        nonce_value: u64,
        sequence: Option<DrcAccountSequenceSelector>,
        ctx: &TxAuthContext,
    ) -> DrcOfferCancelTx {
        let version = if sequence.is_some_and(|s| s.kind == agora_types::DrcAccountSequence::Ticket)
        {
            DRC_OFFER_CANCEL_TICKET_VERSION
        } else {
            DRC_OFFER_CANCEL_TX_VERSION
        };
        let mut tx = DrcOfferCancelTx {
            version,
            submitter: submitter.address(),
            offer_id,
            fee: Amount::from_base_units(1),
            nonce: nonce_value,
            account_sequence: sequence,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_offer_cancel_bound(&mut tx, signer, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    fn apply(
        store: &StateStore,
        payout: &KeyPair,
        creates: Vec<DrcOfferCreateTx>,
        cancels: Vec<DrcOfferCancelTx>,
        blue: u64,
    ) -> Result<(crate::apply::UtxoJournal, BlockAcceptanceRecord), StateError> {
        let ctx = auth();
        let mut block = coinbase(vec![Hash::ZERO], payout);
        block.drc_offer_creates = creates;
        block.drc_offer_cancels = cancels;
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(&ctx), blue)?;
        store.write_batch(result.batch)?;
        crate::store_acceptance(store, &block.id(), &result.acceptance)?;
        Ok((result.journal, result.acceptance))
    }

    fn issue(store: &StateStore, issuer: &KeyPair, holder: &KeyPair, amount: u64, blue: u64) {
        let ctx = auth();
        let cur = std_code(b"USD");
        let mut block = coinbase(vec![Hash::ZERO], issuer);
        block.drc_issued_transfers.push(signed_issued_transfer(
            issuer,
            holder.address(),
            issuer,
            cur,
            amount,
            1,
            nonce(store, issuer),
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(store, block, blue, &ctx);
    }

    fn book_native_for_usd(usd: IssuedAssetId) -> DrcOfferBook {
        DrcOfferBook {
            pays: issued(usd),
            gets: native(),
        }
    }

    #[test]
    fn identical_books_and_native_pair_reject() {
        let owner = key(1);
        let usd = issued(asset(&key(2), std_code(b"USD")));
        let native_native = DrcOfferCreateTx {
            version: DRC_OFFER_CREATE_TX_VERSION,
            owner: owner.address(),
            taker_pays: native(),
            taker_pays_amount: 1,
            taker_gets: native(),
            taker_gets_amount: 1,
            fill_mode: SELL,
            time_in_force: GTC,
            fee: Amount::from_base_units(1),
            expires_after_blue_score: None,
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(native_native.validate_structure().is_err());
        let mut same = native_native;
        same.taker_pays = usd;
        same.taker_gets = usd;
        assert!(same.validate_structure().is_err());
    }

    #[test]
    fn no_cross_rests_in_quality_order_and_pagination_is_not_a_fill() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(1);
        let alice = key(2);
        let bob = key(3);
        let cur = std_code(b"USD");
        setup_live_line(&store, &alice, &issuer, cur, 1_000, 10);
        setup_live_line(&store, &bob, &issuer, cur, 1_000, 11);
        let usd = asset(&issuer, cur);
        issue(&store, &issuer, &alice, 100, 12);
        issue(&store, &issuer, &bob, 100, 13);
        let before_root = compose_trident_state_root(&store, &Hash::ZERO).unwrap();
        let before_burn = burned(&store);

        let worse = signed_create(
            &alice,
            &alice,
            issued(usd),
            20,
            native(),
            10,
            SELL,
            GTC,
            None,
            nonce(&store, &alice),
            None,
            &ctx,
        );
        apply(&store, &alice, vec![worse.clone()], vec![], 20).unwrap();
        let better = signed_create(
            &bob,
            &bob,
            issued(usd),
            10,
            native(),
            10,
            SELL,
            GTC,
            None,
            nonce(&store, &bob),
            None,
            &ctx,
        );
        apply(&store, &bob, vec![better.clone()], vec![], 21).unwrap();
        let book = book_native_for_usd(usd);
        let page = list_book_offers(&store, book, None, 1, 21).unwrap();
        assert_eq!(page.offers.len(), 1);
        assert_eq!(page.offers[0].offer.owner, bob.address());
        assert_eq!(page.offers[0].taker_gets_funded, 10);
        assert!(
            page.next_cursor.is_some(),
            "page={page:?} alice={:?} bob={:?}",
            load_drc_offer_live(&store, &worse.offer_id()).unwrap(),
            load_drc_offer_live(&store, &better.offer_id()).unwrap()
        );
        let rest = list_book_offers(&store, book, page.next_cursor, 8, 21).unwrap();
        assert_eq!(rest.offers.len(), 1);
        assert_eq!(rest.offers[0].offer.owner, alice.address());
        assert!(rest.next_cursor.is_none());
        let stale = list_account_offers(
            &store,
            &alice.address(),
            Some(DrcOfferCursor::from_offer_id(Hash::ZERO)),
            8,
            21,
        );
        assert!(stale
            .unwrap_err()
            .to_string()
            .contains("stale DRC offer cursor"));
        assert_ne!(
            compose_trident_state_root(&store, &Hash::ZERO).unwrap(),
            before_root
        );
        assert_eq!(burned(&store), before_burn + 2);
        assert!(load_drc_offer_live(&store, &better.offer_id())
            .unwrap()
            .is_some());
    }

    #[test]
    fn crossing_partial_multi_maker_self_cross_ioc_and_fok() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(4);
        let maker_a = key(5);
        let maker_b = key(6);
        let taker = key(7);
        let cur = std_code(b"USD");
        for holder in [&maker_a, &maker_b, &taker] {
            setup_live_line(&store, holder, &issuer, cur, 1_000, 10);
        }
        let usd = asset(&issuer, cur);
        issue(&store, &issuer, &taker, 100, 20);
        fund(&store, &maker_a, 50);
        fund(&store, &maker_b, 50);
        fund(&store, &taker, 50);
        let maker_a_before = balance(&store, &maker_a);

        let a = signed_create(
            &maker_a,
            &maker_a,
            issued(usd),
            10,
            native(),
            10,
            SELL,
            GTC,
            None,
            nonce(&store, &maker_a),
            None,
            &ctx,
        );
        apply(&store, &maker_a, vec![a.clone()], vec![], 30).unwrap();
        let b = signed_create(
            &maker_b,
            &maker_b,
            issued(usd),
            20,
            native(),
            10,
            SELL,
            GTC,
            None,
            nonce(&store, &maker_b),
            None,
            &ctx,
        );
        apply(&store, &maker_b, vec![b.clone()], vec![], 31).unwrap();

        let buy = signed_create(
            &taker,
            &taker,
            native(),
            15,
            issued(usd),
            30,
            BUY,
            IOC,
            None,
            nonce(&store, &taker),
            None,
            &ctx,
        );
        let (_journal, acceptance) = apply(&store, &taker, vec![buy.clone()], vec![], 32).unwrap();
        assert_eq!(
            acceptance.drc_offer_create_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        let receipt = crate::load_drc_offer_create_receipt(&store, &buy.offer_id())
            .unwrap()
            .unwrap();
        assert!(receipt.fully_filled, "{receipt:?}");
        assert!(!receipt.placed);
        assert_eq!(receipt.taker_received, 15);
        assert_eq!(receipt.taker_paid, 20);
        assert!(load_drc_offer_live(&store, &a.offer_id())
            .unwrap()
            .is_none());
        let rest_b = load_drc_offer_live(&store, &b.offer_id()).unwrap().unwrap();
        assert_eq!(rest_b.taker_gets_remaining, 5);
        assert_eq!(rest_b.taker_pays_remaining, 10);
        assert_eq!(line_balance(&store, taker.address(), &usd), 80);
        assert_eq!(balance(&store, &maker_a), maker_a_before - 1 - 10);

        let fok = signed_create(
            &taker,
            &taker,
            native(),
            10,
            issued(usd),
            10,
            BUY,
            FOK,
            None,
            nonce(&store, &taker),
            None,
            &ctx,
        );
        let burned_before = burned(&store);
        let nonce_before = nonce(&store, &taker);
        let err = apply(&store, &taker, vec![fok], vec![], 33).unwrap_err();
        assert!(err.to_string().contains("fill-or-kill"));
        assert_eq!(burned(&store), burned_before);
        assert_eq!(nonce(&store, &taker), nonce_before);
        assert!(load_drc_offer_live(&store, &b.offer_id())
            .unwrap()
            .is_some());

        let own = signed_create(
            &maker_b,
            &maker_b,
            native(),
            1,
            issued(usd),
            2,
            SELL,
            GTC,
            None,
            nonce(&store, &maker_b),
            None,
            &ctx,
        );
        let (_j, acc) = apply(&store, &maker_b, vec![own.clone()], vec![], 35).unwrap();
        assert_eq!(
            acc.drc_offer_create_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        let self_receipt = crate::load_drc_offer_create_receipt(&store, &own.offer_id())
            .unwrap()
            .unwrap();
        assert_eq!(self_receipt.self_cross_cancelled, 1);
        assert!(load_drc_offer_live(&store, &b.offer_id())
            .unwrap()
            .is_none());
        assert!(load_drc_offer_live(&store, &own.offer_id())
            .unwrap()
            .is_some());
    }

    #[test]
    fn cancel_idempotence_reorg_and_unfunded_reject() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(8);
        let other = key(9);
        fund(&store, &other, 20);
        let usd = asset(&key(10), std_code(b"USD"));
        setup_live_line(&store, &owner, &key(10), std_code(b"USD"), 100, 10);
        fund(&store, &owner, 100);
        let before_lock = balance(&store, &owner);
        let create = signed_create(
            &owner,
            &owner,
            issued(usd),
            5,
            native(),
            5,
            SELL,
            GTC,
            None,
            nonce(&store, &owner),
            None,
            &ctx,
        );
        let (journal, _) = apply(&store, &owner, vec![create.clone()], vec![], 20).unwrap();
        assert_eq!(balance(&store, &owner), before_lock - 1 - 5);
        let root_live = compose_trident_state_root(&store, &Hash::ZERO).unwrap();
        crate::drc_trust_line_test_harness::support::revert_journal(&store, &journal);
        assert!(load_drc_offer_live(&store, &create.offer_id())
            .unwrap()
            .is_none());
        assert_ne!(
            compose_trident_state_root(&store, &Hash::ZERO).unwrap(),
            root_live
        );
        let (journal, _) = apply(&store, &owner, vec![create.clone()], vec![], 21).unwrap();
        assert!(load_drc_offer_live(&store, &create.offer_id())
            .unwrap()
            .is_some());
        let _ = journal;

        let foreign = signed_cancel(
            &other,
            &other,
            create.offer_id(),
            nonce(&store, &other),
            None,
            &ctx,
        );
        let burned_before = burned(&store);
        assert!(apply(&store, &other, vec![], vec![foreign], 22)
            .unwrap_err()
            .to_string()
            .contains("not the owner"));
        assert_eq!(burned(&store), burned_before);

        let cancel = signed_cancel(
            &owner,
            &owner,
            create.offer_id(),
            nonce(&store, &owner),
            None,
            &ctx,
        );
        apply(&store, &owner, vec![], vec![cancel.clone()], 23).unwrap();
        assert!(load_drc_offer_live(&store, &create.offer_id())
            .unwrap()
            .is_none());
        assert_eq!(
            load_drc_offer_cancel_receipt(&store, &cancel.cancel_tx_id())
                .unwrap()
                .unwrap()
                .outcome,
            DrcOfferCancelOutcome::Removed
        );
        let again = signed_cancel(
            &owner,
            &owner,
            create.offer_id(),
            nonce(&store, &owner),
            None,
            &ctx,
        );
        apply(&store, &owner, vec![], vec![again.clone()], 24).unwrap();
        assert_eq!(
            load_drc_offer_cancel_receipt(&store, &again.cancel_tx_id())
                .unwrap()
                .unwrap()
                .outcome,
            DrcOfferCancelOutcome::Absent
        );

        let poor = key(11);
        fund(&store, &poor, 1);
        let unfunded = signed_create(
            &poor,
            &poor,
            issued(usd),
            1,
            native(),
            5,
            SELL,
            GTC,
            None,
            nonce(&store, &poor),
            None,
            &ctx,
        );
        let burned_before = burned(&store);
        assert!(apply(&store, &poor, vec![unfunded], vec![], 25)
            .unwrap_err()
            .to_string()
            .contains("unfunded"));
        assert_eq!(burned(&store), burned_before);
        assert_eq!(nonce(&store, &poor), 0);
    }

    #[test]
    fn issued_reserve_blocks_transfer_and_clawback_and_issuer_sells() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(12);
        let holder = key(13);
        let buyer = key(14);
        let cur = std_code(b"USD");
        setup_live_line(&store, &holder, &issuer, cur, 100, 10);
        setup_live_line(&store, &buyer, &issuer, cur, 100, 11);
        let usd = asset(&issuer, cur);
        let mut policy_block = coinbase(vec![Hash::ZERO], &issuer);
        policy_block
            .drc_issued_asset_policy_sets
            .push(signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableClawback,
                1,
                nonce(&store, &issuer),
                &ctx,
            ));
        policy_block.header.tx_root = policy_block.compute_body_root();
        apply_block(&store, policy_block, 12, &ctx);
        issue(&store, &issuer, &holder, 10, 13);
        let sell = signed_create(
            &holder,
            &holder,
            native(),
            8,
            issued(usd),
            8,
            SELL,
            GTC,
            None,
            nonce(&store, &holder),
            None,
            &ctx,
        );
        apply(&store, &holder, vec![sell], vec![], 14).unwrap();
        assert_eq!(
            load_issued_offer_reserve(&store, &holder.address(), &usd).unwrap(),
            8
        );
        let mut block = coinbase(vec![Hash::ZERO], &holder);
        block.drc_issued_transfers.push(signed_issued_transfer(
            &holder,
            buyer.address(),
            &issuer,
            cur,
            3,
            1,
            nonce(&store, &holder),
            &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        let err =
            match apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 15) {
                Err(err) => err,
                Ok(_) => panic!("reserved issued transfer was accepted"),
            };
        assert!(err.to_string().contains("reserved by a DRC offer"));
        let claw = signed_clawback(
            &issuer,
            holder.address(),
            cur,
            3,
            1,
            nonce(&store, &issuer),
            &ctx,
        );
        let mut claw_block = coinbase(vec![Hash::ZERO], &issuer);
        claw_block.drc_issued_clawbacks.push(claw);
        claw_block.header.tx_root = claw_block.compute_body_root();
        let err = match apply_block_batched_with_auth_at_blue_score(
            &store,
            &claw_block,
            50,
            Some(&ctx),
            16,
        ) {
            Err(err) => err,
            Ok(_) => panic!("clawback of reserved units was accepted"),
        };
        assert!(err.to_string().contains("reserved by a DRC offer"));
        let claw_ok = signed_clawback(
            &issuer,
            holder.address(),
            cur,
            2,
            1,
            nonce(&store, &issuer),
            &ctx,
        );
        let mut claw_ok_block = coinbase(vec![Hash::ZERO], &issuer);
        claw_ok_block.drc_issued_clawbacks.push(claw_ok);
        claw_ok_block.header.tx_root = claw_ok_block.compute_body_root();
        apply_block(&store, claw_ok_block, 17, &ctx);
        assert_eq!(line_balance(&store, holder.address(), &usd), 8);

        let issuer_sell = signed_create(
            &issuer,
            &issuer,
            native(),
            2,
            issued(usd),
            4,
            SELL,
            GTC,
            None,
            nonce(&store, &issuer),
            None,
            &ctx,
        );
        apply(&store, &issuer, vec![issuer_sell.clone()], vec![], 18).unwrap();
        assert_eq!(
            load_issued_offer_reserve(&store, &issuer.address(), &usd).unwrap(),
            0
        );
        let buy = signed_create(
            &buyer,
            &buyer,
            issued(usd),
            4,
            native(),
            4,
            BUY,
            GTC,
            None,
            nonce(&store, &buyer),
            None,
            &ctx,
        );
        apply(&store, &buyer, vec![buy], vec![], 19).unwrap();
        assert!(load_drc_offer_live(&store, &issuer_sell.offer_id())
            .unwrap()
            .is_none());
        assert_eq!(line_balance(&store, buyer.address(), &usd), 4);
        assert_eq!(issuer_outstanding(&store, &usd), 12);
    }

    #[test]
    fn freeze_deep_freeze_require_auth_and_native_is_not_issuer_controlled() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer = key(15);
        let seller = key(16);
        let buyer = key(17);
        let cur = std_code(b"USD");
        setup_live_line(&store, &seller, &issuer, cur, 100, 10);
        setup_live_line(&store, &buyer, &issuer, cur, 100, 11);
        let usd = asset(&issuer, cur);
        issue(&store, &issuer, &seller, 20, 12);
        let resting = signed_create(
            &seller,
            &seller,
            native(),
            5,
            issued(usd),
            5,
            SELL,
            GTC,
            None,
            nonce(&store, &seller),
            None,
            &ctx,
        );
        apply(&store, &seller, vec![resting.clone()], vec![], 13).unwrap();
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
                1,
                nonce(&store, &issuer),
                &ctx,
            ),
            &ctx,
            14,
        )
        .unwrap();
        let taker = signed_create(
            &buyer,
            &buyer,
            issued(usd),
            5,
            native(),
            5,
            BUY,
            IOC,
            None,
            nonce(&store, &buyer),
            None,
            &ctx,
        );
        apply(&store, &buyer, vec![taker], vec![], 15).unwrap();
        assert!(load_drc_offer_live(&store, &resting.offer_id())
            .unwrap()
            .is_some());
        assert_eq!(line_balance(&store, buyer.address(), &usd), 0);
        apply_policy_direct(
            &store,
            &signed_policy_set(
                &issuer,
                cur,
                DrcIssuedAssetPolicyAction::ClearGlobalFreeze,
                1,
                nonce(&store, &issuer),
                &ctx,
            ),
            &ctx,
            16,
        )
        .unwrap();
        apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                seller.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineFrozen(true),
                1,
                nonce(&store, &issuer),
                &ctx,
            ),
            &ctx,
            17,
        )
        .unwrap();
        apply_issuer_control_direct(
            &store,
            &signed_issuer_control(
                &issuer,
                seller.address(),
                cur,
                DrcTrustLineIssuerControlAction::SetLineDeepFrozen(true),
                1,
                nonce(&store, &issuer),
                &ctx,
            ),
            &ctx,
            18,
        )
        .unwrap();
        let sweep = signed_create(
            &buyer,
            &buyer,
            issued(usd),
            5,
            native(),
            5,
            BUY,
            IOC,
            None,
            nonce(&store, &buyer),
            None,
            &ctx,
        );
        apply(&store, &buyer, vec![sweep], vec![], 19).unwrap();
        assert!(load_drc_offer_live(&store, &resting.offer_id())
            .unwrap()
            .is_none());
        let blocked = signed_create(
            &seller,
            &seller,
            native(),
            1,
            issued(usd),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &seller),
            None,
            &ctx,
        );
        assert!(apply(&store, &seller, vec![blocked], vec![], 20)
            .unwrap_err()
            .to_string()
            .contains("unfunded"));

        let native_seller = key(18);
        fund(&store, &native_seller, 30);
        let native_offer = signed_create(
            &native_seller,
            &native_seller,
            issued(usd),
            1,
            native(),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &native_seller),
            None,
            &ctx,
        );
        apply(
            &store,
            &native_seller,
            vec![native_offer.clone()],
            vec![],
            21,
        )
        .unwrap();
        assert_eq!(
            load_drc_offer_live(&store, &native_offer.offer_id())
                .unwrap()
                .unwrap()
                .native_locked,
            1
        );
    }

    #[test]
    fn auth_matrix_ticket_regular_key_master_disable_and_multisign() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(19);
        let regular = key(20);
        let signer = key(21);
        let usd_issuer = key(22);
        let cur = std_code(b"EUR");
        fund(&store, &owner, 80);
        setup_live_line(&store, &owner, &usd_issuer, cur, 50, 10);
        let pays = issued(asset(&usd_issuer, cur));

        install_signer_list(&store, &owner, &[(&signer, 1)], &ctx);
        let ticket = mint_ticket(&store, &owner);
        let ticketed = signed_create(
            &owner,
            &owner,
            pays,
            2,
            native(),
            2,
            SELL,
            GTC,
            None,
            0,
            Some(DrcAccountSequenceSelector::ticket(ticket)),
            &ctx,
        );
        apply(&store, &owner, vec![ticketed.clone()], vec![], 30).unwrap();
        assert!(load_drc_offer_live(&store, &ticketed.offer_id())
            .unwrap()
            .is_some());
        let replay = signed_create(
            &owner,
            &owner,
            pays,
            1,
            native(),
            1,
            SELL,
            GTC,
            None,
            0,
            Some(DrcAccountSequenceSelector::ticket(ticket)),
            &ctx,
        );
        assert!(apply(&store, &owner, vec![replay], vec![], 31).is_err());

        let mut set = DrcRegularKeyTx::set(
            owner.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::from_base_units(1),
            nonce(&store, &owner),
        );
        sign_drc_regular_key_bound(&mut set, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = crate::store::WriteBatch::new();
        let mut journal = crate::AccountJournal::default();
        crate::apply_drc_regular_key(&store, &set, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let by_regular = signed_create(
            &owner,
            &regular,
            pays,
            2,
            native(),
            2,
            SELL,
            GTC,
            None,
            nonce(&store, &owner),
            None,
            &ctx,
        );
        apply(&store, &owner, vec![by_regular.clone()], vec![], 32).unwrap();
        assert!(load_drc_offer_live(&store, &by_regular.offer_id())
            .unwrap()
            .is_some());

        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            owner.address(),
            Amount::from_base_units(1),
            nonce(&store, &owner),
        );
        sign_drc_account_policy_bound(&mut disable, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = crate::store::WriteBatch::new();
        let mut journal = crate::AccountJournal::default();
        crate::apply_drc_account_policy(&store, &disable, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let by_master = signed_create(
            &owner,
            &owner,
            pays,
            1,
            native(),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &owner),
            None,
            &ctx,
        );
        assert!(apply(&store, &owner, vec![by_master], vec![], 33).is_err());
        let by_regular_again = signed_create(
            &owner,
            &regular,
            pays,
            1,
            native(),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &owner),
            None,
            &ctx,
        );
        apply(&store, &owner, vec![by_regular_again], vec![], 34).unwrap();

        let mut multi = DrcOfferCreateTx {
            version: DRC_OFFER_CREATE_TX_VERSION,
            owner: owner.address(),
            taker_pays: pays,
            taker_pays_amount: 1,
            taker_gets: native(),
            taker_gets_amount: 1,
            fill_mode: SELL,
            time_in_force: GTC,
            fee: Amount::from_base_units(1),
            expires_after_blue_score: None,
            nonce: nonce(&store, &owner),
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let bytes = multi.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        multi.multisign = Some(multisign_bundle(
            owner.address(),
            &bytes,
            &[(&signer, 1)],
            &ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_offer_creates.push(multi.clone());
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 40)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            result.acceptance.drc_offer_create_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert!(load_drc_offer_live(&store, &multi.offer_id())
            .unwrap()
            .is_some());
    }

    #[test]
    fn expiration_dust_account_cap_and_distinct_issuers() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let issuer_a = key(30);
        let issuer_b = key(31);
        let maker = key(32);
        let taker = key(33);
        let cur = std_code(b"USD");
        setup_live_line(&store, &maker, &issuer_a, cur, 100, 10);
        setup_live_line(&store, &taker, &issuer_a, cur, 100, 11);
        let mut block = coinbase(vec![Hash::ZERO], &taker);
        block.drc_trust_line_sets.push(
            crate::drc_trust_line_test_harness::support::signed_trust_line_set(
                &taker,
                &issuer_b,
                cur,
                100,
                1,
                nonce(&store, &taker),
                &ctx,
            ),
        );
        block.header.tx_root = block.compute_body_root();
        apply_block(&store, block, 12, &ctx);
        let usd_a = asset(&issuer_a, cur);
        let usd_b = asset(&issuer_b, cur);
        issue(&store, &issuer_a, &maker, 10, 13);
        fund(&store, &maker, 20);
        fund(&store, &taker, 20);

        let expired = signed_create(
            &maker,
            &maker,
            native(),
            4,
            issued(usd_a),
            4,
            SELL,
            GTC,
            Some(15),
            nonce(&store, &maker),
            None,
            &ctx,
        );
        apply(&store, &maker, vec![expired.clone()], vec![], 15).unwrap();
        let sweep = signed_create(
            &taker,
            &taker,
            issued(usd_a),
            4,
            native(),
            4,
            BUY,
            IOC,
            None,
            nonce(&store, &taker),
            None,
            &ctx,
        );
        apply(&store, &taker, vec![sweep], vec![], 16).unwrap();
        assert!(load_drc_offer_live(&store, &expired.offer_id())
            .unwrap()
            .is_none());
        assert_eq!(line_balance(&store, maker.address(), &usd_a), 10);

        let other_issuer = signed_create(
            &taker,
            &taker,
            issued(usd_b),
            1,
            native(),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &taker),
            None,
            &ctx,
        );
        apply(&store, &taker, vec![other_issuer.clone()], vec![], 17).unwrap();
        let page = list_book_offers(
            &store,
            DrcOfferBook {
                pays: native(),
                gets: issued(usd_a),
            },
            None,
            8,
            17,
        )
        .unwrap();
        assert!(page.offers.is_empty());
        assert!(load_drc_offer_live(&store, &other_issuer.offer_id())
            .unwrap()
            .is_some());

        let dusty = signed_create(
            &maker,
            &maker,
            issued(usd_a),
            100,
            native(),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &maker),
            None,
            &ctx,
        );
        apply(&store, &maker, vec![dusty.clone()], vec![], 18).unwrap();
        issue(&store, &issuer_a, &taker, 50, 19);
        let taker_dust = signed_create(
            &taker,
            &taker,
            native(),
            1,
            issued(usd_a),
            100,
            BUY,
            IOC,
            None,
            nonce(&store, &taker),
            None,
            &ctx,
        );
        let (_j, _) = apply(&store, &taker, vec![taker_dust], vec![], 20).unwrap();
        assert!(load_drc_offer_live(&store, &dusty.offer_id())
            .unwrap()
            .is_none());

        let capped = key(34);
        fund(&store, &capped, 80);
        for _ in 0..32 {
            let tx = signed_create(
                &capped,
                &capped,
                issued(usd_b),
                1,
                native(),
                1,
                SELL,
                GTC,
                None,
                nonce(&store, &capped),
                None,
                &ctx,
            );
            apply(&store, &capped, vec![tx], vec![], 40).unwrap();
        }
        let overflow = signed_create(
            &capped,
            &capped,
            issued(usd_b),
            1,
            native(),
            1,
            SELL,
            GTC,
            None,
            nonce(&store, &capped),
            None,
            &ctx,
        );
        let burned_before = burned(&store);
        assert!(apply(&store, &capped, vec![overflow], vec![], 41)
            .unwrap_err()
            .to_string()
            .contains("live offer cap"));
        assert_eq!(burned(&store), burned_before);
        assert_eq!(
            list_account_offers(&store, &capped.address(), None, 32, 41)
                .unwrap()
                .offers
                .len(),
            32
        );
    }
}
