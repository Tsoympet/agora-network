//! Shared helpers for DRC ticket integration tests (Stage C/D).

#[cfg(test)]
pub mod support {
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{sign_drc_ticket_create_bound, KeyPair};
    use agora_types::{
        Amount, Block, BlockHeader, DrcTicketCreateTx, Hash, NativeAssetId, Transaction, TxOut,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_ticket::{drc_ticket_root, load_drc_account_tickets};
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::supply::{
        load_burned_supply, load_issued_supply, put_burned_supply_into, put_issued_supply_into,
        put_schema_version_into,
    };
    use crate::{StateStore, SCHEMA_VERSION};

    pub const TIP: Hash = Hash([4; 32]);

    static COINBASE_SEQ: AtomicU64 = AtomicU64::new(2_000_000);

    pub fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    pub fn key(b: u8) -> KeyPair {
        KeyPair::from_secret_bytes(&[b; 32]).unwrap()
    }

    pub fn fund(store: &StateStore, kp: &KeyPair, amount: u64) {
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
        for asset in NativeAssetId::ALL {
            put_burned_supply_into(&mut batch, asset, load_burned_supply(store, asset).unwrap());
        }
        put_schema_version_into(&mut batch, SCHEMA_VERSION);
        store.write_batch(batch).unwrap();
    }

    pub fn coinbase(parents: Vec<Hash>, payout: &KeyPair) -> Block {
        let seq = COINBASE_SEQ.fetch_add(1, Ordering::Relaxed);
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents,
                timestamp_ms: seq,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![Transaction::unsigned(
                1,
                vec![],
                vec![TxOut {
                    value: Amount::from_base_units(50),
                    address: payout.address(),
                }],
                seq,
            )],
        );
        block.header.tx_root = block.compute_body_root();
        block
    }

    pub fn signed_create(owner: &KeyPair, nonce: u64, ctx: &TxAuthContext) -> DrcTicketCreateTx {
        let mut tx = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, nonce);
        sign_drc_ticket_create_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn mint_ticket(
        store: &StateStore,
        owner: &KeyPair,
        ctx: &TxAuthContext,
        parents: Vec<Hash>,
    ) -> (u64, Hash) {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let create = signed_create(owner, nonce, ctx);
        let seq = nonce + 1;
        let mut block = coinbase(parents, owner);
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), 50).unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(load_drc_account_tickets(store, &owner.address())
            .unwrap()
            .contains(&seq));
        (seq, block.id())
    }

    pub struct TicketSnapshot {
        pub tickets: Vec<u64>,
        pub nonce: u64,
        pub ticket_root: Hash,
        pub state_root: Hash,
        pub balance: u64,
    }

    pub fn snapshot_ticket_state(store: &StateStore, owner: &KeyPair) -> TicketSnapshot {
        crate::reindex_drc_ledger_objects(store).unwrap();
        TicketSnapshot {
            tickets: load_drc_account_tickets(store, &owner.address()).unwrap(),
            nonce: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            ticket_root: drc_ticket_root(store).unwrap(),
            state_root: compose_trident_state_root(store, &TIP).unwrap(),
            balance: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
        }
    }

    pub fn assert_ticket_snapshot_unchanged(
        store: &StateStore,
        owner: &KeyPair,
        before: &TicketSnapshot,
    ) {
        assert_eq!(
            load_drc_account_tickets(store, &owner.address()).unwrap(),
            before.tickets
        );
        assert_eq!(
            load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            before.nonce
        );
        assert_eq!(drc_ticket_root(store).unwrap(), before.ticket_root);
        assert_eq!(
            compose_trident_state_root(store, &TIP).unwrap(),
            before.state_root
        );
        assert_eq!(
            load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            before.balance
        );
    }

    pub fn reject_block_preserving_ticket_state(
        store: &StateStore,
        owner: &KeyPair,
        block: &Block,
        ctx: &TxAuthContext,
        before: &TicketSnapshot,
    ) {
        assert!(
            apply_block_batched_with_auth_at_blue_score(store, block, 50, Some(ctx), 50).is_err()
        );
        assert_ticket_snapshot_unchanged(store, owner, before);
    }

    pub fn reject_invalid_multisign_or_apply_preserving_ticket_state(
        store: &StateStore,
        owner: &KeyPair,
        block: &Block,
        ctx: &TxAuthContext,
        before: &TicketSnapshot,
    ) {
        let invalid_lane =
            agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
                .is_err();
        let invalid_apply =
            apply_block_batched_with_auth_at_blue_score(store, block, 50, Some(ctx), 50).is_err();
        assert!(invalid_lane || invalid_apply);
        assert_ticket_snapshot_unchanged(store, owner, before);
    }
}
