//! Shared helpers for native DRC payment channel core tests.

#[cfg(test)]
pub mod support {
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{
        sign_drc_payment_channel_claim_bound, sign_drc_payment_channel_close_bound,
        sign_drc_payment_channel_create_bound, sign_drc_payment_channel_fund_bound,
        sign_payment_channel_offledger_claim, KeyPair,
    };
    use agora_types::{
        Amount, Block, BlockHeader, DrcPaymentChannelClaimTx, DrcPaymentChannelCloseKind,
        DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, Hash,
        NativeAssetId, Transaction, TxOut, DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION,
        DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION, DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
        DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_payment_channel::{drc_payment_channel_root, load_drc_payment_channel_live};
    use crate::store::WriteBatch;
    use crate::StateStore;

    pub const TIP: Hash = Hash([4; 32]);
    static COINBASE_SEQ: AtomicU64 = AtomicU64::new(4_000_000);

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

    pub fn signed_create(
        owner: &KeyPair,
        claim_key: &KeyPair,
        destination: agora_types::Address,
        amount: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcPaymentChannelCreateTx {
        let mut tx = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
            owner: owner.address(),
            destination,
            amount: Amount::from_base_units(amount),
            fee: Amount::from_base_units(1),
            claim_public_key: claim_key.public_key_bytes().to_vec(),
            settle_delay_blue_scores: 5,
            destination_tag: Some(0),
            source_tag: None,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score: None,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_create_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn apply_channel_block(
        store: &StateStore,
        mut block: Block,
        blue_score: u64,
        ctx: &TxAuthContext,
    ) {
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue_score)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        crate::acceptance::store_acceptance(store, &block.id(), &result.acceptance).unwrap();
    }

    pub fn create_live_channel(
        store: &StateStore,
        owner: &KeyPair,
        claim_key: &KeyPair,
        dest: &KeyPair,
        amount: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> (Hash, DrcPaymentChannelCreateTx) {
        let create = signed_create(owner, claim_key, dest.address(), amount, nonce, ctx);
        let channel_id = create.channel_id();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_payment_channel_creates.push(create.clone());
        apply_channel_block(store, block, 1, ctx);
        assert!(load_drc_payment_channel_live(store, &channel_id)
            .unwrap()
            .is_some());
        (channel_id, create)
    }

    pub fn signed_fund(
        owner: &KeyPair,
        channel_id: Hash,
        amount: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcPaymentChannelFundTx {
        let mut tx = DrcPaymentChannelFundTx {
            version: DRC_PAYMENT_CHANNEL_FUND_TX_VERSION,
            submitter: owner.address(),
            channel_id,
            amount: Amount::from_base_units(amount),
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_fund_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_claim(
        dest: &KeyPair,
        claim_key: &KeyPair,
        channel_id: Hash,
        cumulative: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcPaymentChannelClaimTx {
        let offledger = sign_payment_channel_offledger_claim(
            claim_key,
            &ctx.chain_id,
            &ctx.genesis,
            &channel_id,
            Amount::from_base_units(cumulative),
        )
        .unwrap();
        let mut tx = DrcPaymentChannelClaimTx {
            version: DRC_PAYMENT_CHANNEL_CLAIM_TX_VERSION,
            submitter: dest.address(),
            channel_id,
            cumulative_authorized: Amount::from_base_units(cumulative),
            channel_claim_signature: offledger.to_vec(),
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_claim_bound(&mut tx, dest, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_close(
        submitter: &KeyPair,
        channel_id: Hash,
        kind: DrcPaymentChannelCloseKind,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcPaymentChannelCloseTx {
        let mut tx = DrcPaymentChannelCloseTx {
            version: DRC_PAYMENT_CHANNEL_CLOSE_TX_VERSION,
            submitter: submitter.address(),
            channel_id,
            close_kind: kind,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_payment_channel_close_bound(&mut tx, submitter, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        tx
    }

    pub fn channel_root(store: &StateStore) -> Hash {
        drc_payment_channel_root(store).unwrap()
    }
}
