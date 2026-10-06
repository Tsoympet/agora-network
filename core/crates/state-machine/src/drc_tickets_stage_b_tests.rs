//! Stage-B admission policy: mempool planning, RPC lookup semantics, security matrix slices.

#[cfg(test)]
mod tests {
    use agora_crypto::{sign_drc_payment_bound, sign_drc_ticket_create_bound, KeyPair};
    use agora_types::{
        Amount, DrcAccountSequenceSelector, DrcPaymentTx, DrcTicketCreateTx, Hash, NativeAssetId,
        DRC_PAYMENT_TICKET_VERSION,
    };

    use crate::accounts::credit_account_into;
    use crate::drc_mempool::{
        lookup_drc_ticket_point, plan_drc_mempool_reservation, DrcTicketPointStatus,
    };
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::store::WriteBatch;
    use crate::{apply_drc_ticket_create, AccountJournal, StateStore, TxAuthContext};

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
    fn public_admission_rejects_ticket_spend_without_canonical_live() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        fund(&store, &owner, 100);
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(2).address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(1));
        sign_drc_payment_bound(&mut payment, &owner, &auth().chain_id, &auth().genesis).unwrap();
        assert!(plan_drc_mempool_reservation(
            &store,
            &owner.address(),
            payment.version,
            DRC_PAYMENT_TICKET_VERSION,
            payment.nonce,
            payment.account_sequence,
        )
        .is_err());
    }

    #[test]
    fn canonical_ticket_lookup_live_after_create() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        fund(&store, &owner, 200);
        let ctx = auth();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, 0);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        apply_drc_ticket_create(&store, &create, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            lookup_drc_ticket_point(&store, &owner.address(), 1).unwrap(),
            DrcTicketPointStatus::Live
        );
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![1]
        );
    }

    #[test]
    fn mempool_ticket_spend_allowed_after_canonical_create() {
        let store = StateStore::open_in_memory();
        let owner = key(4);
        fund(&store, &owner, 200);
        let ctx = auth();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, 0);
        sign_drc_ticket_create_bound(&mut create, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        apply_drc_ticket_create(&store, &create, &ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert_eq!(
            plan_drc_mempool_reservation(
                &store,
                &owner.address(),
                DRC_PAYMENT_TICKET_VERSION,
                DRC_PAYMENT_TICKET_VERSION,
                0,
                Some(DrcAccountSequenceSelector::ticket(1)),
            )
            .unwrap(),
            crate::drc_mempool::DrcMempoolReservation::Ticket { sequence: 1 }
        );
    }
}
