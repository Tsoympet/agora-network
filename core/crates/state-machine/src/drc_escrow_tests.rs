//! Native DRC escrow security and semantics tests.

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_drc_escrow_cancel_bound, sign_drc_escrow_create_bound, sign_drc_escrow_finish_bound,
        KeyPair,
    };
    use agora_types::{
        escrow_cancel_allowed, escrow_finish_allowed, validate_escrow_time_bounds, Amount,
        DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx, DrcEscrowOutcome, Hash,
        NativeAssetId, DRC_ESCROW_CREATE_TX_VERSION, DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::TxAuthContext;
    use crate::drc_escrow::{
        apply_drc_escrow_cancel, apply_drc_escrow_create, apply_drc_escrow_finish, drc_escrow_root,
        load_drc_escrow_live, load_drc_escrow_receipt, lookup_drc_escrow_point,
    };
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

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
        store.write_batch(batch).unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    fn signed_create(
        owner: &KeyPair,
        recipient: agora_types::Address,
        amount: u64,
        finish: Option<u64>,
        cancel: Option<u64>,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcEscrowCreateTx {
        let mut tx = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: owner.address(),
            recipient,
            amount: Amount::from_base_units(amount),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            finish_after_blue_score: finish,
            cancel_after_blue_score: cancel,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_create_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    #[test]
    fn pinned_time_bound_validation() {
        assert!(validate_escrow_time_bounds(None, None).is_err());
        assert!(validate_escrow_time_bounds(Some(10), Some(10)).is_err());
        assert!(validate_escrow_time_bounds(Some(11), Some(10)).is_err());
        assert!(validate_escrow_time_bounds(Some(5), Some(10)).is_ok());
        assert!(!escrow_finish_allowed(9, Some(10), Some(20)));
        assert!(escrow_finish_allowed(10, Some(10), Some(20)));
        assert!(!escrow_finish_allowed(20, Some(10), Some(20)));
        assert!(!escrow_cancel_allowed(19, Some(20)));
        assert!(escrow_cancel_allowed(20, Some(20)));
    }

    #[test]
    fn create_locks_funds_and_live_query() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        let recipient = key(2);
        fund(&store, &owner, 1_000);
        let ctx = auth();
        let create = signed_create(&owner, recipient.address(), 100, Some(5), Some(50), 0, &ctx);
        let id = create.escrow_id();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(lookup_drc_escrow_point(&store, &id).unwrap(), "live");
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            899
        );
        assert!(load_drc_escrow_live(&store, &id).unwrap().is_some());
    }

    #[test]
    fn finish_before_window_rejected() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        let recipient = key(4);
        fund(&store, &owner, 500);
        let ctx = auth();
        let create = signed_create(&owner, recipient.address(), 50, Some(100), None, 0, &ctx);
        let id = create.escrow_id();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let mut finish = DrcEscrowFinishTx {
            version: agora_types::DRC_ESCROW_FINISH_TX_VERSION,
            submitter: owner.address(),
            escrow_id: id,
            fee: Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_finish_bound(&mut finish, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(
            apply_drc_escrow_finish(&store, &finish, &ctx, 99, &mut batch, &mut journal).is_err()
        );
    }

    #[test]
    fn finish_and_cancel_exactly_once() {
        let store = StateStore::open_in_memory();
        let owner = key(5);
        let recipient = key(6);
        let helper = key(7);
        fund(&store, &owner, 800);
        fund(&store, &helper, 100);
        let ctx = auth();
        let create = signed_create(
            &owner,
            recipient.address(),
            200,
            Some(10),
            Some(100),
            0,
            &ctx,
        );
        let id = create.escrow_id();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut finish = DrcEscrowFinishTx {
            version: agora_types::DRC_ESCROW_FINISH_TX_VERSION,
            submitter: helper.address(),
            escrow_id: id,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_finish_bound(&mut finish, &helper, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let receipt =
            apply_drc_escrow_finish(&store, &finish, &ctx, 10, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(receipt.outcome, DrcEscrowOutcome::Finished);
        assert_eq!(lookup_drc_escrow_point(&store, &id).unwrap(), "unknown");
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            200
        );
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(
            apply_drc_escrow_finish(&store, &finish, &ctx, 11, &mut batch, &mut journal).is_err()
        );
    }

    #[test]
    fn cancel_after_window_returns_owner() {
        let store = StateStore::open_in_memory();
        let owner = key(8);
        let recipient = key(9);
        fund(&store, &owner, 600);
        let ctx = auth();
        let create = signed_create(&owner, recipient.address(), 150, None, Some(30), 0, &ctx);
        let id = create.escrow_id();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let balance_after_create = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .balance;

        let mut cancel = DrcEscrowCancelTx {
            version: agora_types::DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: owner.address(),
            escrow_id: id,
            fee: Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_cancel_bound(&mut cancel, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let receipt =
            apply_drc_escrow_cancel(&store, &cancel, &ctx, 30, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(receipt.outcome, DrcEscrowOutcome::Cancelled);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            balance_after_create
                .checked_add(create.amount.as_base_units())
                .unwrap()
        );
        assert!(load_drc_escrow_receipt(&store, &id).unwrap().is_some());
    }

    #[test]
    fn live_escrow_cap_enforced() {
        let store = StateStore::open_in_memory();
        let owner = key(10);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let recipient = key(11).address();
        let mut nonce = 0u64;
        for i in 0..DRC_MAX_LIVE_ESCROWS_PER_ACCOUNT {
            let create = signed_create(
                &owner,
                recipient,
                1,
                None,
                Some(100 + i as u64),
                nonce,
                &ctx,
            );
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
            store.write_batch(batch).unwrap();
            nonce += 1;
        }
        let fail = signed_create(&owner, recipient, 1, None, Some(9999), nonce, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_escrow_create(&store, &fail, &ctx, 1, &mut batch, &mut journal).is_err());
    }

    #[test]
    fn third_party_cancel_submitter_semantics() {
        let store = StateStore::open_in_memory();
        let owner = key(16);
        let recipient = key(17);
        let stranger = key(18);
        fund(&store, &owner, 900);
        fund(&store, &stranger, 50);
        let ctx = auth();
        let create = signed_create(&owner, recipient.address(), 100, None, Some(25), 0, &ctx);
        let id = create.escrow_id();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let owner_after = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .balance;

        let mut cancel = DrcEscrowCancelTx {
            version: agora_types::DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: stranger.address(),
            escrow_id: id,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_cancel_bound(&mut cancel, &stranger, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_cancel(&store, &cancel, &ctx, 25, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            owner_after + 100
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &stranger.address())
                .unwrap()
                .balance,
            49
        );
    }

    #[test]
    fn borsh_roundtrip_create_finish_cancel() {
        use borsh::BorshDeserialize;
        let owner = key(12);
        let ctx = auth();
        let create = signed_create(&owner, key(13).address(), 9, Some(1), Some(2), 0, &ctx);
        let bytes = borsh::to_vec(&create).unwrap();
        let decoded = DrcEscrowCreateTx::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.escrow_id(), create.escrow_id());
    }

    #[test]
    fn escrow_root_changes_with_live_state() {
        let store = StateStore::open_in_memory();
        let tip = Hash([3; 32]);
        let before = compose_trident_state_root(&store, &tip).unwrap();
        let owner = key(14);
        fund(&store, &owner, 500);
        let ctx = auth();
        let create = signed_create(&owner, key(15).address(), 10, None, Some(5), 0, &ctx);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_escrow_create(&store, &create, &ctx, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let after = compose_trident_state_root(&store, &tip).unwrap();
        assert_ne!(before, after);
        assert_ne!(
            drc_escrow_root(&store).unwrap(),
            Hash::hash_borsh(&(b"x", 1))
        );
    }
}
