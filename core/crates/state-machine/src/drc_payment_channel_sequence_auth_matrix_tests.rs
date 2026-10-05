//! Nonce, Ticket, and master/regular/multisign authorization matrices for payment channels.

#[cfg(test)]
mod shared {
    use agora_crypto::sign_drc_ticket_create_bound;
    use agora_types::{DrcTicketCreateTx, Hash, NativeAssetId};

    use crate::apply::apply_block_batched_with_auth_at_blue_score;
    use crate::drc_payment_channel_test_harness::support::{auth, coinbase, fund, key};
    use crate::store::WriteBatch;
    use crate::StateStore;

    pub fn mint_ticket(store: &StateStore, owner: &agora_crypto::KeyPair) -> u64 {
        let ctx = auth();
        let nonce = crate::accounts::load_account(store, NativeAssetId::DRC, &owner.address())
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
                apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(&ctx), 50)
                    .unwrap()
                    .batch,
            )
            .unwrap();
        nonce + 1
    }

    pub fn setup_live_channel(
        store: &StateStore,
        cancel_after: Option<u64>,
    ) -> (
        agora_crypto::KeyPair,
        agora_crypto::KeyPair,
        agora_crypto::KeyPair,
        Hash,
    ) {
        let ctx = auth();
        let owner = key(1);
        let dest = key(2);
        let claim_key = key(3);
        fund(store, &owner, 500_000);
        fund(store, &dest, 500);
        let nonce = crate::accounts::load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let create = super::super::drc_payment_channel_test_harness::support::signed_create(
            &owner,
            &claim_key,
            dest.address(),
            200,
            nonce,
            &ctx,
            Some(0),
            None,
            cancel_after,
            5,
        );
        let channel_id = create.channel_id();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_creates.push(create);
        super::super::drc_payment_channel_test_harness::support::apply_channel_block(
            store, block, 10, &ctx,
        );
        (owner, dest, claim_key, channel_id)
    }
}

#[cfg(test)]
mod ticket_matrix {
    use agora_crypto::{
        sign_drc_payment_channel_claim_bound, sign_drc_payment_channel_close_bound,
        sign_drc_payment_channel_create_bound, sign_drc_payment_channel_fund_bound,
        sign_drc_ticket_create_bound,
    };
    use agora_types::{
        DrcAccountSequenceSelector, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind,
        DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx,
        DrcTicketCreateTx, Hash, DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION,
        DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION, DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION,
        DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION, DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION,
        DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION, DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
        DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
    };

    use crate::accounts::load_account;
    use crate::drc_mempool::lookup_drc_ticket_point;
    use crate::drc_payment_channel::{
        apply_drc_payment_channel_claim, apply_drc_payment_channel_close,
        apply_drc_payment_channel_create, apply_drc_payment_channel_fund,
    };
    use crate::drc_payment_channel_test_harness::support::{
        apply_channel_block, auth, coinbase, create_live_channel, fund, key,
        reject_channel_block_preserving_invariants, signed_claim, signed_close, signed_fund,
        snapshot_channel_invariants,
    };
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::NativeAssetId;

    use super::shared::{mint_ticket, setup_live_channel};

    #[test]
    fn ordinary_nonce_create_fund_claim_close_each_advances_submitter_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(10);
        let dest = key(11);
        let claim_key = key(12);
        fund(&store, &owner, 50_000);
        fund(&store, &dest, 20);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            1
        );
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block
            .drc_payment_channel_funds
            .push(signed_fund(&owner, channel_id, 10, 1, &ctx));
        apply_channel_block(&store, block, 2, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            2
        );
        let dest_nonce = load_account(&store, NativeAssetId::DRC, &dest.address())
            .unwrap()
            .nonce;
        let mut block2 = coinbase(vec![Hash::ZERO], &dest);
        block2.drc_payment_channel_claims.push(signed_claim(
            &dest, &claim_key, channel_id, 30, dest_nonce, &ctx,
        ));
        apply_channel_block(&store, block2, 3, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &dest.address())
                .unwrap()
                .nonce,
            dest_nonce + 1
        );
        let owner_nonce = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut block3 = coinbase(vec![Hash::ZERO], &owner);
        block3.drc_payment_channel_closes.push(signed_close(
            &owner,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            owner_nonce,
            &ctx,
        ));
        apply_channel_block(&store, block3, 4, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            owner_nonce + 1
        );
    }

    #[test]
    fn ticket_create_fund_claim_close_one_use_each_preserves_account_nonce() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, claim_key, channel_id) = setup_live_channel(&store, Some(500));
        let seq_create = mint_ticket(&store, &owner);
        let owner_nonce_before = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut create = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION,
            owner: owner.address(),
            destination: key(99).address(),
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            claim_public_key: key(100).public_key_bytes().to_vec(),
            settle_delay_blue_scores: 3,
            destination_tag: Some(0),
            source_tag: None,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score: Some(600),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq_create)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_creates.push(create);
        apply_channel_block(&store, block, 11, &ctx);
        assert!(!load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq_create));
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            owner_nonce_before
        );

        let seq_fund = mint_ticket(&store, &owner);
        let mut fund_tx = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            amount: agora_types::Amount::from_base_units(10),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq_fund)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut fund_tx, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block2 = coinbase(vec![Hash::ZERO], &owner);
        block2.drc_payment_channel_funds.push(fund_tx);
        apply_channel_block(&store, block2, 12, &ctx);
        assert!(!load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq_fund));

        let seq_claim = mint_ticket(&store, &dest);
        let dest_nonce_before = load_account(&store, NativeAssetId::DRC, &dest.address())
            .unwrap()
            .nonce;
        let offledger = agora_crypto::sign_payment_channel_offledger_claim(
            &claim_key,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            agora_types::Amount::from_base_units(40),
        )
        .unwrap();
        let mut claim = DrcPaymentChannelClaimTx {
            version: DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION,
            submitter: dest.address(),
            channel_id,
            cumulative_authorized: agora_types::Amount::from_base_units(40),
            channel_claim_signature: offledger.to_vec(),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq_claim)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_claim_bound(&mut claim, &dest, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block3 = coinbase(vec![Hash::ZERO], &dest);
        block3.drc_payment_channel_claims.push(claim);
        apply_channel_block(&store, block3, 13, &ctx);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &dest.address())
                .unwrap()
                .nonce,
            dest_nonce_before
        );

        let seq_close = mint_ticket(&store, &owner);
        let mut close = DrcPaymentChannelCloseTx {
            version: DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            close_kind: DrcPaymentChannelCloseKind::OwnerScheduleClose,
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq_close)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_close_bound(&mut close, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block4 = coinbase(vec![Hash::ZERO], &owner);
        block4.drc_payment_channel_closes.push(close);
        apply_channel_block(&store, block4, 14, &ctx);
        assert!(!load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq_close));
    }

    #[test]
    fn unknown_ticket_rejected_on_create_preserves_tickets() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(20);
        fund(&store, &owner, 100_000);
        let before = snapshot_channel_invariants(&store, &owner, &key(21), &Hash::ZERO);
        let mut create = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION,
            owner: owner.address(),
            destination: key(21).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::from_base_units(1),
            claim_public_key: key(22).public_key_bytes().to_vec(),
            settle_delay_blue_scores: 2,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score: Some(100),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(999)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        let after = snapshot_channel_invariants(&store, &owner, &key(21), &Hash::ZERO);
        assert_eq!(after.owner_tickets, before.owner_tickets);
        assert_eq!(
            lookup_drc_ticket_point(&store, &owner.address(), 999).unwrap(),
            crate::drc_mempool::DrcTicketPointStatus::Unknown
        );
    }

    macro_rules! legacy_rejects_ticket {
        ($name:ident, $version:expr) => {
            #[test]
            fn $name() {
                let store = StateStore::open_in_memory();
                let owner = key(30);
                fund(&store, &owner, 100_000);
                let seq = mint_ticket(&store, &owner);
                let tx = DrcPaymentChannelCreateTx {
                    version: $version,
                    owner: owner.address(),
                    destination: key(31).address(),
                    amount: agora_types::Amount::from_base_units(1),
                    fee: agora_types::Amount::ZERO,
                    claim_public_key: key(32).public_key_bytes().to_vec(),
                    settle_delay_blue_scores: 2,
                    destination_tag: None,
                    source_tag: None,
                    invoice_id: Hash::ZERO,
                    cancel_after_blue_score: Some(50),
                    nonce: 0,
                    account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
                    public_key: Vec::new(),
                    signature: Vec::new(),
                    multisign: None,
                };
                assert!(tx.validate_structure().is_err());
            }
        };
    }

    legacy_rejects_ticket!(
        legacy_create_version_rejects_ticket_selector,
        DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION
    );
    legacy_rejects_ticket!(
        legacy_fund_version_rejects_ticket_selector,
        DRC_PAYMENT_CHANNEL_FUND_TX_VERSION
    );
    legacy_rejects_ticket!(
        legacy_claim_version_rejects_ticket_selector,
        DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION
    );
    legacy_rejects_ticket!(
        legacy_close_version_rejects_ticket_selector,
        DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION
    );

    #[test]
    fn future_create_version_rejects_ticket_selector() {
        let seq = {
            let store = StateStore::open_in_memory();
            let owner = key(33);
            fund(&store, &owner, 100);
            mint_ticket(&store, &owner)
        };
        let create = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION + 1,
            owner: key(33).address(),
            destination: key(34).address(),
            amount: agora_types::Amount::from_base_units(1),
            fee: agora_types::Amount::ZERO,
            claim_public_key: key(35).public_key_bytes().to_vec(),
            settle_delay_blue_scores: 2,
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score: Some(50),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        assert!(create.validate_structure().is_err());
    }

    #[test]
    fn reused_ticket_rejected_second_fund_preserves_ticket_set() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, _claim_key, channel_id) = setup_live_channel(&store, None);
        let seq = mint_ticket(&store, &owner);
        let mut fund_tx = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut fund_tx, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payment_channel_funds.push(fund_tx);
        apply_channel_block(&store, block, 20, &ctx);
        let mut fund2 = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            amount: agora_types::Amount::from_base_units(3),
            fee: agora_types::Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut fund2, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(
            apply_drc_payment_channel_fund(&store, &fund2, &ctx, 21, &mut batch, &mut journal)
                .is_err()
        );
        assert_channel_invariants_unchanged_helper(&store, &owner, &dest, &channel_id, &before);
    }

    #[test]
    fn cross_owner_ticket_fund_rejected_preserves_victim_ticket() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, _claim_key, channel_id) = setup_live_channel(&store, None);
        let other = key(40);
        fund(&store, &other, 50_000);
        let seq = mint_ticket(&store, &other);
        let mut fund_tx = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            amount: agora_types::Amount::from_base_units(2),
            fee: agora_types::Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut fund_tx, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_fund(
            &store,
            &fund_tx,
            &ctx,
            22,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_invariants_unchanged_helper(&store, &owner, &dest, &channel_id, &before);
        assert!(load_drc_account_tickets(&store, &other.address())
            .unwrap()
            .contains(&seq));
    }

    #[test]
    fn selector_tamper_after_sign_fund_rejects_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, _claim_key, channel_id) = setup_live_channel(&store, None);
        let seq = mint_ticket(&store, &owner);
        let mut fund_tx = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            amount: agora_types::Amount::from_base_units(2),
            fee: agora_types::Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut fund_tx, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        fund_tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq + 1));
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_fund(
            &store,
            &fund_tx,
            &ctx,
            23,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_invariants_unchanged_helper(&store, &owner, &dest, &channel_id, &before);
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq));
    }

    #[test]
    fn ticket_retained_on_claim_auth_failure_and_cancel_cutoff_and_balance_conflict() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, claim_key, channel_id) = setup_live_channel(&store, Some(100));
        let seq = mint_ticket(&store, &dest);
        let offledger = agora_crypto::sign_payment_channel_offledger_claim(
            &claim_key,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            agora_types::Amount::from_base_units(50),
        )
        .unwrap();
        let mut claim = DrcPaymentChannelClaimTx {
            version: DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION,
            submitter: dest.address(),
            channel_id,
            cumulative_authorized: agora_types::Amount::from_base_units(50),
            channel_claim_signature: offledger.to_vec(),
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_claim_bound(&mut claim, &dest, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        assert!(!claim.signature.is_empty());
        claim.signature[0] ^= 0xff;
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_claim(
            &store,
            &claim,
            &ctx,
            50,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_invariants_unchanged_helper(&store, &owner, &dest, &channel_id, &before);
        assert!(load_drc_account_tickets(&store, &dest.address())
            .unwrap()
            .contains(&seq));

        let mut claim_cutoff = claim.clone();
        claim_cutoff.signature.clear();
        sign_drc_payment_channel_claim_bound(&mut claim_cutoff, &dest, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        assert!(apply_drc_payment_channel_claim(
            &store,
            &claim_cutoff,
            &ctx,
            100,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert!(load_drc_account_tickets(&store, &dest.address())
            .unwrap()
            .contains(&seq));

        let mut batch2 = WriteBatch::new();
        let mut acct = load_account(&store, NativeAssetId::DRC, &dest.address()).unwrap();
        acct.balance = 0;
        crate::accounts::put_account_into(&mut batch2, NativeAssetId::DRC, &dest.address(), &acct)
            .unwrap();
        store.write_batch(batch2).unwrap();
        assert!(apply_drc_payment_channel_claim(
            &store,
            &claim_cutoff,
            &ctx,
            50,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert!(load_drc_account_tickets(&store, &dest.address())
            .unwrap()
            .contains(&seq));
    }

    #[test]
    fn ticket_close_rejected_before_finalize_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let (owner, dest, _claim_key, channel_id) = setup_live_channel(&store, None);
        let seq = mint_ticket(&store, &owner);
        let mut close = DrcPaymentChannelCloseTx {
            version: DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION,
            submitter: owner.address(),
            channel_id,
            close_kind: DrcPaymentChannelCloseKind::Finalize,
            fee: agora_types::Amount::from_base_units(1),
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_close_bound(&mut close, &owner, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let before = snapshot_channel_invariants(&store, &owner, &dest, &channel_id);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_close(
            &store,
            &close,
            &ctx,
            15,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_invariants_unchanged_helper(&store, &owner, &dest, &channel_id, &before);
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .contains(&seq));
    }

    fn assert_channel_invariants_unchanged_helper(
        store: &StateStore,
        owner: &agora_crypto::KeyPair,
        dest: &agora_crypto::KeyPair,
        channel_id: &Hash,
        before: &crate::drc_payment_channel_test_harness::support::ChannelInvariantSnapshot,
    ) {
        use crate::drc_payment_channel_test_harness::support::assert_channel_invariants_unchanged;
        assert_channel_invariants_unchanged(store, owner, dest, channel_id, before);
    }
}

#[cfg(test)]
mod master_auth {
    use agora_crypto::{
        sign_drc_account_policy_bound, sign_drc_payment_channel_create_bound,
        sign_drc_payment_channel_fund_bound, sign_drc_regular_key_bound,
        sign_payment_channel_offledger_claim,
    };
    use agora_types::{DrcAccountPolicyTx, DrcPaymentChannelFundTx, DrcRegularKeyTx};

    use crate::accounts::load_account;
    use crate::drc_payment_channel::{
        apply_drc_payment_channel_claim, apply_drc_payment_channel_create,
        apply_drc_payment_channel_fund,
    };
    use crate::drc_payment_channel_test_harness::multisign::{
        install_signer_list, multisign_bundle, multisign_fund_block,
    };
    use crate::drc_payment_channel_test_harness::support::{
        assert_channel_snapshot_unchanged, auth, create_live_channel, fund, key, signed_claim,
        signed_close, signed_create, snapshot_channel_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_types::{DrcPaymentChannelCloseKind, NativeAssetId};

    fn disable_master(store: &StateStore, master: &agora_crypto::KeyPair) {
        let ctx = auth();
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            agora_types::Amount::ZERO,
            load_account(store, NativeAssetId::DRC, &master.address())
                .unwrap()
                .nonce,
        );
        sign_drc_account_policy_bound(&mut disable, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_policy::apply_drc_account_policy(
            store,
            &disable,
            &ctx,
            &mut batch,
            &mut journal,
        )
        .unwrap();
        store.write_batch(batch).unwrap();
    }

    fn install_regular(
        store: &StateStore,
        master: &agora_crypto::KeyPair,
        regular: &agora_crypto::KeyPair,
    ) {
        let ctx = auth();
        let reg_nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut reg = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            agora_types::Amount::ZERO,
            reg_nonce,
        );
        sign_drc_regular_key_bound(&mut reg, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::drc_regular_key::apply_drc_regular_key(store, &reg, &ctx, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn disabled_master_create_rejected_regular_key_apply_succeeds() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let master = key(50);
        let regular = key(51);
        let dest = key(52);
        let claim_key = key(53);
        fund(&store, &master, 200_000);
        install_regular(&store, &master, &regular);
        disable_master(&store, &master);
        let before = snapshot_channel_state(&store, &master, &dest);
        let nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = signed_create(
            &master,
            &claim_key,
            dest.address(),
            20,
            nonce,
            &ctx,
            Some(0),
            None,
            Some(200),
            5,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_snapshot_unchanged(&store, &master, &dest, &before);
        sign_drc_payment_channel_create_bound(&mut create, &regular, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_create(
            &store,
            &create,
            &ctx,
            1,
            &mut batch,
            &mut journal
        )
        .is_ok());
    }

    #[test]
    fn disabled_master_close_rejected_regular_key_succeeds() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let master = key(54);
        let regular = key(55);
        let dest = key(56);
        let claim_key = key(57);
        fund(&store, &master, 200_000);
        fund(&store, &dest, 5);
        install_regular(&store, &master, &regular);
        let (channel_id, _) = create_live_channel(&store, &master, &claim_key, &dest, 40, 0, &ctx);
        disable_master(&store, &master);
        let before = snapshot_channel_state(&store, &master, &dest);
        let mut close = signed_close(
            &master,
            channel_id,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            load_account(&store, NativeAssetId::DRC, &master.address())
                .unwrap()
                .nonce,
            &ctx,
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_payment_channel::apply_drc_payment_channel_close(
            &store,
            &close,
            &ctx,
            2,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_snapshot_unchanged(&store, &master, &dest, &before);
        agora_crypto::sign_drc_payment_channel_close_bound(
            &mut close,
            &regular,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(crate::drc_payment_channel::apply_drc_payment_channel_close(
            &store,
            &close,
            &ctx,
            2,
            &mut batch,
            &mut journal
        )
        .is_ok());
    }

    #[test]
    fn multisign_fund_positive_after_signer_list() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let master = key(60);
        let dest = key(61);
        let s1 = key(62);
        fund(&store, &master, 100_000);
        let (id, _) = create_live_channel(&store, &master, &key(63), &dest, 40, 0, &ctx);
        let block = multisign_fund_block(&store, &master, id, &[(&s1, 1)], &ctx);
        crate::drc_payment_channel_test_harness::support::apply_channel_block(
            &store, block, 5, &ctx,
        );
    }

    #[test]
    fn stale_signer_list_fund_rejected() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let master = key(64);
        let dest = key(65);
        let s1 = key(66);
        fund(&store, &master, 100_000);
        let (id, _) = create_live_channel(&store, &master, &key(67), &dest, 40, 0, &ctx);
        install_signer_list(&store, &master, &[(&s1, 1)], &ctx);
        let block = multisign_fund_block(&store, &master, id, &[(&s1, 1)], &ctx);
        install_signer_list(&store, &master, &[(&key(68), 1)], &ctx);
        let before = snapshot_channel_state(&store, &master, &dest);
        crate::drc_payment_channel_test_harness::multisign::reject_preserving(
            &store, &master, &dest, &block, &ctx, &before, 50,
        );
    }

    #[test]
    fn claim_multisign_on_chain_does_not_replace_mandatory_offledger_signature() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let owner = key(70);
        let dest = key(71);
        let claim_key = key(72);
        let s1 = key(73);
        fund(&store, &owner, 100_000);
        fund(&store, &dest, 50);
        let (channel_id, _) = create_live_channel(&store, &owner, &claim_key, &dest, 100, 0, &ctx);
        install_signer_list(&store, &dest, &[(&s1, 1)], &ctx);
        let bad_offledger = sign_payment_channel_offledger_claim(
            &claim_key,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            agora_types::Amount::from_base_units(999),
        )
        .unwrap();
        let mut claim = signed_claim(&dest, &claim_key, channel_id, 40, 0, &ctx);
        claim.channel_claim_signature = bad_offledger.to_vec();
        claim.public_key.clear();
        claim.signature.clear();
        claim.multisign = Some(multisign_bundle(
            dest.address(),
            &claim.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            &[(&s1, 1)],
            &ctx,
        ));
        let before = snapshot_channel_state(&store, &owner, &dest);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_claim(
            &store,
            &claim,
            &ctx,
            10,
            &mut batch,
            &mut journal
        )
        .is_err());
        assert_channel_snapshot_unchanged(&store, &owner, &dest, &before);
    }

    #[test]
    fn recovery_regular_key_fund_after_master_disabled() {
        let store = StateStore::open_in_memory();
        let ctx = auth();
        let master = key(74);
        let regular = key(75);
        let dest = key(76);
        fund(&store, &master, 100_000);
        let (id, _) = create_live_channel(&store, &master, &key(77), &dest, 30, 0, &ctx);
        install_regular(&store, &master, &regular);
        disable_master(&store, &master);
        let fund_nonce = load_account(&store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut fund_tx = DrcPaymentChannelFundTx {
            version: agora_types::DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
            submitter: master.address(),
            channel_id: id,
            amount: agora_types::Amount::from_base_units(5),
            fee: agora_types::Amount::from_base_units(1),
            nonce: fund_nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut fund_tx, &regular, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_payment_channel_fund(
            &store,
            &fund_tx,
            &ctx,
            3,
            &mut batch,
            &mut journal
        )
        .is_ok());
    }
}
