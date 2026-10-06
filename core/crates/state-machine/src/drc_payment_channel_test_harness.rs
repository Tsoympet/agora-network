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
    use crate::drc_payment_channel::{
        drc_payment_channel_root, load_drc_payment_channel_live, payment_channel_owner_index_key,
    };
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::supply::{
        load_burned_supply, load_issued_supply, put_burned_supply_into, put_issued_supply_into,
        put_schema_version_into,
    };
    use crate::{StateStore, SCHEMA_VERSION};

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

    pub fn signed_create_simple(
        owner: &KeyPair,
        claim_key: &KeyPair,
        destination: agora_types::Address,
        amount: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcPaymentChannelCreateTx {
        signed_create(
            owner,
            claim_key,
            destination,
            amount,
            nonce,
            ctx,
            Some(0),
            None,
            None,
            5,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn signed_create(
        owner: &KeyPair,
        claim_key: &KeyPair,
        destination: agora_types::Address,
        amount: u64,
        nonce: u64,
        ctx: &TxAuthContext,
        destination_tag: Option<u32>,
        source_tag: Option<u32>,
        cancel_after_blue_score: Option<u64>,
        settle_delay_blue_scores: u64,
    ) -> DrcPaymentChannelCreateTx {
        let mut tx = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
            owner: owner.address(),
            destination,
            amount: Amount::from_base_units(amount),
            fee: Amount::from_base_units(1),
            claim_public_key: claim_key.public_key_bytes().to_vec(),
            settle_delay_blue_scores,
            destination_tag,
            source_tag,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score,
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
    ) -> Hash {
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue_score)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        crate::store_acceptance(store, &block.id(), &result.acceptance).unwrap();
        block.id()
    }

    pub fn apply_block_capture(
        store: &StateStore,
        mut block: Block,
        blue_score: u64,
        ctx: &TxAuthContext,
    ) -> (
        Hash,
        crate::apply::UtxoJournal,
        crate::BlockAcceptanceRecord,
    ) {
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue_score)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        crate::store_acceptance(store, &block.id(), &result.acceptance).unwrap();
        (block.id(), result.journal, result.acceptance)
    }

    pub fn revert_journal(store: &StateStore, journal: &crate::apply::UtxoJournal) {
        store
            .write_batch(crate::apply::revert_journal_batched(journal).unwrap())
            .unwrap();
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
        create_live_channel_at_score(store, owner, claim_key, dest, amount, nonce, ctx, 1)
    }

    pub fn create_live_channel_at_score(
        store: &StateStore,
        owner: &KeyPair,
        claim_key: &KeyPair,
        dest: &KeyPair,
        amount: u64,
        _nonce: u64,
        ctx: &TxAuthContext,
        blue_score: u64,
    ) -> (Hash, DrcPaymentChannelCreateTx) {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let create = signed_create_simple(owner, claim_key, dest.address(), amount, nonce, ctx);
        let channel_id = create.channel_id();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_payment_channel_creates.push(create.clone());
        apply_channel_block(store, block, blue_score, ctx);
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

    pub fn count_live_channels(store: &StateStore, owner: &agora_types::Address) -> usize {
        use crate::columns::ColumnFamily;
        use borsh::BorshDeserialize;
        let key = payment_channel_owner_index_key(owner);
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

    pub fn locked_channel_total(store: &StateStore, owner: &agora_types::Address) -> u64 {
        use crate::columns::ColumnFamily;
        use borsh::BorshDeserialize;
        let key = payment_channel_owner_index_key(owner);
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
                load_drc_payment_channel_live(store, id)
                    .unwrap()
                    .unwrap()
                    .total_funded
                    .as_base_units()
                    - load_drc_payment_channel_live(store, id)
                        .unwrap()
                        .unwrap()
                        .cumulative_claimed
                        .as_base_units()
            })
            .sum()
    }

    pub struct ChannelSnapshot {
        pub owner_balance: u64,
        pub destination_balance: u64,
        pub owner_nonce: u64,
        pub destination_nonce: u64,
        pub channel_root: Hash,
        pub state_root: Hash,
        pub live_count: usize,
    }

    pub fn snapshot_channel_live_cumulative(store: &StateStore, channel_id: &Hash) -> u64 {
        load_drc_payment_channel_live(store, channel_id)
            .unwrap()
            .map(|l| l.cumulative_claimed.as_base_units())
            .unwrap_or(0)
    }

    pub struct ChannelInvariantSnapshot {
        pub base: ChannelSnapshot,
        pub owner_tickets: Vec<u64>,
        pub destination_tickets: Vec<u64>,
        pub live_cumulative: u64,
    }

    pub fn snapshot_channel_invariants(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        channel_id: &Hash,
    ) -> ChannelInvariantSnapshot {
        use crate::drc_ticket::load_drc_account_tickets;
        ChannelInvariantSnapshot {
            base: snapshot_channel_state(store, owner, destination),
            owner_tickets: load_drc_account_tickets(store, &owner.address()).unwrap(),
            destination_tickets: load_drc_account_tickets(store, &destination.address()).unwrap(),
            live_cumulative: snapshot_channel_live_cumulative(store, channel_id),
        }
    }

    pub fn assert_channel_invariants_unchanged(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        channel_id: &Hash,
        before: &ChannelInvariantSnapshot,
    ) {
        assert_channel_snapshot_unchanged(store, owner, destination, &before.base);
        let after = snapshot_channel_invariants(store, owner, destination, channel_id);
        assert_eq!(after.owner_tickets, before.owner_tickets);
        assert_eq!(after.destination_tickets, before.destination_tickets);
        assert_eq!(after.live_cumulative, before.live_cumulative);
    }

    pub fn reject_channel_block_preserving_invariants(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        channel_id: &Hash,
        block: &Block,
        ctx: &TxAuthContext,
        before: &ChannelInvariantSnapshot,
        blue_score: u64,
    ) {
        if agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
            .is_err()
        {
            assert_channel_invariants_unchanged(store, owner, destination, channel_id, before);
            return;
        }
        assert!(apply_block_batched_with_auth_at_blue_score(
            store,
            block,
            50,
            Some(ctx),
            blue_score
        )
        .is_err());
        assert_channel_invariants_unchanged(store, owner, destination, channel_id, before);
    }

    pub fn snapshot_channel_state(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
    ) -> ChannelSnapshot {
        crate::reindex_drc_ledger_objects(store).unwrap();
        ChannelSnapshot {
            owner_balance: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            destination_balance: load_account(store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance,
            owner_nonce: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            destination_nonce: load_account(store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .nonce,
            channel_root: channel_root(store),
            state_root: compose_trident_state_root(store, &TIP).unwrap(),
            live_count: count_live_channels(store, &owner.address()),
        }
    }

    pub fn assert_channel_snapshot_unchanged(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        before: &ChannelSnapshot,
    ) {
        let after = snapshot_channel_state(store, owner, destination);
        assert_eq!(after.owner_balance, before.owner_balance);
        assert_eq!(after.destination_balance, before.destination_balance);
        assert_eq!(after.owner_nonce, before.owner_nonce);
        assert_eq!(after.destination_nonce, before.destination_nonce);
        assert_eq!(after.channel_root, before.channel_root);
        assert_eq!(after.state_root, before.state_root);
        assert_eq!(after.live_count, before.live_count);
    }

    pub fn reject_channel_apply_preserving_state(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        block: &Block,
        ctx: &TxAuthContext,
        before: &ChannelSnapshot,
        blue_score: u64,
    ) {
        if agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
            .is_err()
        {
            assert_channel_snapshot_unchanged(store, owner, destination, before);
            return;
        }
        assert!(apply_block_batched_with_auth_at_blue_score(
            store,
            block,
            50,
            Some(ctx),
            blue_score
        )
        .is_err());
        assert_channel_snapshot_unchanged(store, owner, destination, before);
    }

    pub fn spendable_plus_locked(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
    ) -> u64 {
        load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .balance
            + load_account(store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance
            + locked_channel_total(store, &owner.address())
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub mod multisign {
    use agora_crypto::{sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair};
    use agora_types::{
        materialize_drc_multisign_attachments, Amount, Block, DrcMultisignAuth, DrcMultisignEntry,
        DrcPaymentChannelCreateTx, DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId,
        DRC_MULTISIGN_AUTH_VERSION, DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
    };

    use super::support::{
        coinbase, reject_channel_apply_preserving_state,
        reject_channel_block_preserving_invariants, signed_claim, signed_close, signed_fund,
        ChannelInvariantSnapshot, ChannelSnapshot,
    };
    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_signer_list::apply_drc_signer_list;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    pub fn install_signer_list(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) {
        let nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        install_signer_list_at_nonce(store, master, signers, ctx, nonce);
    }

    pub fn install_signer_list_at_nonce(
        store: &StateStore,
        master: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
        nonce: u64,
    ) {
        let total: u16 = signers.iter().map(|(_, w)| w).sum();
        let mut install = DrcSignerListTx::unsigned_set(
            master.address(),
            agora_types::canonical_sorted_entries(
                &signers
                    .iter()
                    .map(|(kp, weight)| DrcSignerListEntry {
                        signer: kp.address(),
                        weight: *weight,
                    })
                    .collect::<Vec<_>>(),
            ),
            u32::from(total),
            Amount::ZERO,
            nonce,
        );
        sign_drc_signer_list_bound(&mut install, master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_signer_list(store, &install, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    pub fn multisign_bundle(
        owner: agora_types::Address,
        signing_bytes: &[u8],
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> DrcMultisignAuth {
        let mut entries = Vec::new();
        for (kp, _) in signers {
            let (signer, public_key, signature) = sign_drc_multisign_participant_bound(
                owner,
                signing_bytes,
                kp,
                &ctx.chain_id,
                &ctx.genesis,
            )
            .unwrap();
            entries.push(DrcMultisignEntry {
                signer,
                public_key,
                signature,
            });
        }
        entries.sort_by_key(|e| e.signer.0);
        DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: owner,
            signatures: entries,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reject_preserving_invariants(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        channel_id: Hash,
        block: &Block,
        ctx: &TxAuthContext,
        before: &ChannelInvariantSnapshot,
        blue_score: u64,
    ) {
        reject_channel_block_preserving_invariants(
            store,
            owner,
            destination,
            &channel_id,
            block,
            ctx,
            before,
            blue_score,
        );
    }

    pub fn reject_preserving(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        block: &Block,
        ctx: &TxAuthContext,
        before: &ChannelSnapshot,
        blue_score: u64,
    ) {
        reject_channel_apply_preserving_state(
            store,
            owner,
            destination,
            block,
            ctx,
            before,
            blue_score,
        );
    }

    pub fn base_multisign_create_block(
        store: &StateStore,
        master: &KeyPair,
        claim_key: &KeyPair,
        destination: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, master, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = DrcPaymentChannelCreateTx {
            version: DRC_PAYMENT_CHANNEL_CREATE_TX_VERSION,
            owner: master.address(),
            destination: destination.address(),
            amount: Amount::from_base_units(10),
            fee: Amount::from_base_units(1),
            claim_public_key: claim_key.public_key_bytes().to_vec(),
            settle_delay_blue_scores: 5,
            destination_tag: Some(0),
            source_tag: None,
            invoice_id: Hash::ZERO,
            cancel_after_blue_score: Some(100),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        create.public_key.clear();
        create.signature.clear();
        create.multisign = Some(multisign_bundle(
            master.address(),
            &create.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], master);
        block.drc_payment_channel_creates.push(create);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn multisign_fund_block(
        store: &StateStore,
        owner: &KeyPair,
        channel_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, owner, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut fund = signed_fund(owner, channel_id, 5, nonce, ctx);
        fund.public_key.clear();
        fund.signature.clear();
        fund.multisign = Some(multisign_bundle(
            owner.address(),
            &fund.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_payment_channel_funds.push(fund);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn multisign_claim_block(
        store: &StateStore,
        destination: &KeyPair,
        claim_key: &KeyPair,
        channel_id: Hash,
        cumulative: u64,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, destination, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let mut claim = signed_claim(destination, claim_key, channel_id, cumulative, nonce, ctx);
        claim.public_key.clear();
        claim.signature.clear();
        claim.multisign = Some(multisign_bundle(
            destination.address(),
            &claim.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], destination);
        block.drc_payment_channel_claims.push(claim);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn multisign_close_block(
        store: &StateStore,
        submitter: &KeyPair,
        channel_id: Hash,
        kind: agora_types::DrcPaymentChannelCloseKind,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, submitter, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &submitter.address())
            .unwrap()
            .nonce;
        let mut close = signed_close(submitter, channel_id, kind, nonce, ctx);
        close.public_key.clear();
        close.signature.clear();
        close.multisign = Some(multisign_bundle(
            submitter.address(),
            &close.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], submitter);
        block.drc_payment_channel_closes.push(close);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn base_multisign_fund_block(
        store: &StateStore,
        owner: &KeyPair,
        channel_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        multisign_fund_block(store, owner, channel_id, signers, ctx)
    }

    pub fn base_multisign_close_block(
        store: &StateStore,
        submitter: &KeyPair,
        channel_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        multisign_close_block(
            store,
            submitter,
            channel_id,
            agora_types::DrcPaymentChannelCloseKind::OwnerScheduleClose,
            signers,
            ctx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn base_multisign_claim_block(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        claim_key: &KeyPair,
        channel_id: Hash,
        cumulative: u64,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        let _ = owner;
        multisign_claim_block(
            store,
            destination,
            claim_key,
            channel_id,
            cumulative,
            signers,
            ctx,
        )
    }
}
