//! DRC Tickets: create, cap, consumption, and ticketed transfer tests.

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_account_transfer_bound, sign_drc_ticket_create_bound, KeyPair};
    use agora_types::{
        Amount, DrcAccountSequenceSelector, DrcTicketCreateTx, Hash, NativeAssetId,
        DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::TxAuthContext;
    use crate::drc_ticket::{apply_drc_ticket_create, load_drc_account_tickets};
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

    #[test]
    fn ticket_create_reserves_sequence_and_advances_nonce_by_two() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        fund(&store, &owner, 100);
        let ctx = auth();
        let mut tx = DrcTicketCreateTx::unsigned(owner.address(), Amount::from_base_units(1), 0);
        sign_drc_ticket_create_bound(&mut tx, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let seq = apply_drc_ticket_create(&store, &tx, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            2
        );
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![1]
        );
    }

    #[test]
    fn ticket_cap_rejects_create_atomically() {
        let store = StateStore::open_in_memory();
        let owner = key(2);
        fund(&store, &owner, 10_000);
        let ctx = auth();
        let mut nonce = 0u64;
        for _ in 0..DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT {
            let mut tx = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, nonce);
            sign_drc_ticket_create_bound(&mut tx, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            apply_drc_ticket_create(&store, &tx, &ctx, &mut batch, &mut journal).unwrap();
            store.write_batch(batch).unwrap();
            nonce += 2;
        }
        let before = load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .len();
        let mut fail = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, nonce);
        sign_drc_ticket_create_bound(&mut fail, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        assert!(apply_drc_ticket_create(&store, &fail, &ctx, &mut batch, &mut journal).is_err());
        assert!(batch.is_empty());
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address())
                .unwrap()
                .len(),
            before
        );
    }

    #[test]
    fn ticketed_transfer_v3_consumes_ticket_not_nonce() {
        use crate::accounts::apply_account_transfer;
        use agora_types::AccountTransfer;

        let store = StateStore::open_in_memory();
        let owner = key(3);
        let peer = key(4);
        fund(&store, &owner, 200);
        let ctx = auth();
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, 0);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_ticket_create(&store, &create, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let nonce_after_create = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;

        let mut transfer = AccountTransfer::unsigned_with_fee_v3(
            NativeAssetId::DRC,
            owner.address(),
            peer.address(),
            Amount::from_base_units(5),
            Amount::ZERO,
            DrcAccountSequenceSelector::ticket(1),
        );
        sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_account_transfer(&store, &transfer, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            nonce_after_create
        );
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .is_empty());
    }
}
