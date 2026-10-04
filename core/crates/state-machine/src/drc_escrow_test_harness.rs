//! Shared helpers for native DRC escrow hardening tests.

#[cfg(test)]
#[allow(dead_code)]
pub mod support {
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{
        sign_drc_escrow_cancel_bound, sign_drc_escrow_create_bound, sign_drc_escrow_finish_bound,
        KeyPair,
    };
    use agora_types::{
        Amount, Block, BlockHeader, DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx, Hash,
        NativeAssetId, Transaction, TxOut, DRC_ESCROW_CANCEL_TX_VERSION,
        DRC_ESCROW_CREATE_TX_VERSION, DRC_ESCROW_FINISH_TX_VERSION,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_escrow::{
        drc_escrow_root, escrow_owner_index_key, lookup_drc_escrow_point,
    };
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::StateStore;

    pub const TIP: Hash = Hash([4; 32]);
    static COINBASE_SEQ: AtomicU64 = AtomicU64::new(3_000_000);

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

    #[allow(clippy::too_many_arguments)]
    pub fn signed_create(
        owner: &KeyPair,
        recipient: agora_types::Address,
        amount: u64,
        finish: Option<u64>,
        cancel: Option<u64>,
        nonce: u64,
        ctx: &TxAuthContext,
        destination_tag: Option<u32>,
    ) -> DrcEscrowCreateTx {
        let mut tx = DrcEscrowCreateTx {
            version: DRC_ESCROW_CREATE_TX_VERSION,
            owner: owner.address(),
            recipient,
            amount: Amount::from_base_units(amount),
            fee: Amount::from_base_units(1),
            destination_tag,
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

    pub fn signed_finish(
        submitter: &KeyPair,
        escrow_id: Hash,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcEscrowFinishTx {
        let mut tx = DrcEscrowFinishTx {
            version: DRC_ESCROW_FINISH_TX_VERSION,
            submitter: submitter.address(),
            escrow_id,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_finish_bound(&mut tx, submitter, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_cancel(
        submitter: &KeyPair,
        escrow_id: Hash,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcEscrowCancelTx {
        let mut tx = DrcEscrowCancelTx {
            version: DRC_ESCROW_CANCEL_TX_VERSION,
            submitter: submitter.address(),
            escrow_id,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_escrow_cancel_bound(&mut tx, submitter, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn apply_escrow_block(
        store: &StateStore,
        mut block: Block,
        blue_score: u64,
        ctx: &TxAuthContext,
    ) -> Hash {
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue_score)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        crate::store_acceptance(store, &block.id(), &result.acceptance).unwrap();
        block.id()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_live_escrow(
        store: &StateStore,
        owner: &KeyPair,
        recipient: &KeyPair,
        amount: u64,
        finish: Option<u64>,
        cancel: Option<u64>,
        create_score: u64,
        ctx: &TxAuthContext,
    ) -> (Hash, DrcEscrowCreateTx) {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let create = signed_create(
            owner,
            recipient.address(),
            amount,
            finish,
            cancel,
            nonce,
            ctx,
            None,
        );
        let id = create.escrow_id();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_escrow_creates.push(create.clone());
        apply_escrow_block(store, block, create_score, ctx);
        assert_eq!(lookup_drc_escrow_point(store, &id).unwrap(), "live");
        (id, create)
    }

    pub struct EscrowSnapshot {
        pub owner_balance: u64,
        pub recipient_balance: u64,
        pub owner_nonce: u64,
        pub escrow_root: Hash,
        pub state_root: Hash,
        pub live_count: usize,
    }

    pub fn snapshot_escrow_state(
        store: &StateStore,
        owner: &KeyPair,
        recipient: &KeyPair,
    ) -> EscrowSnapshot {
        EscrowSnapshot {
            owner_balance: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            recipient_balance: load_account(store, NativeAssetId::DRC, &recipient.address())
                .unwrap()
                .balance,
            owner_nonce: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            escrow_root: drc_escrow_root(store).unwrap(),
            state_root: compose_trident_state_root(store, &TIP).unwrap(),
            live_count: count_live_escrows(store, &owner.address()),
        }
    }

    pub fn count_live_escrows(store: &StateStore, owner: &agora_types::Address) -> usize {
        use crate::columns::ColumnFamily;
        use borsh::BorshDeserialize;
        let key = escrow_owner_index_key(owner);
        let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key).unwrap() else {
            return 0;
        };
        #[derive(BorshDeserialize)]
        struct Idx {
            _v: u32,
            _o: agora_types::Address,
            live_ids: Vec<Hash>,
        }
        Idx::try_from_slice(&bytes)
            .map(|i| i.live_ids.len())
            .unwrap_or(0)
    }

    pub fn load_block_acceptance(
        store: &StateStore,
        block_id: &Hash,
    ) -> crate::BlockAcceptanceRecord {
        crate::load_acceptance(store, block_id)
            .unwrap()
            .expect("acceptance record")
    }

    pub fn total_drc_balances(store: &StateStore, addrs: &[agora_types::Address]) -> u64 {
        addrs
            .iter()
            .map(|a| load_account(store, NativeAssetId::DRC, a).unwrap().balance)
            .sum()
    }

    pub fn locked_escrow_total(store: &StateStore, owner: &agora_types::Address) -> u64 {
        use crate::columns::ColumnFamily;
        use crate::drc_escrow::{escrow_owner_index_key, load_drc_escrow_live};
        use borsh::BorshDeserialize;
        let key = escrow_owner_index_key(owner);
        let Some(bytes) = store.get_cf(ColumnFamily::Meta, &key).unwrap() else {
            return 0;
        };
        #[derive(BorshDeserialize)]
        struct Idx {
            _v: u32,
            _o: agora_types::Address,
            live_ids: Vec<Hash>,
        }
        let idx = Idx::try_from_slice(&bytes).unwrap();
        idx.live_ids
            .iter()
            .map(|id| {
                load_drc_escrow_live(store, id)
                    .unwrap()
                    .unwrap()
                    .amount
                    .as_base_units()
            })
            .sum()
    }
}
