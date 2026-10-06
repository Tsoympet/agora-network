//! Shared helpers for native DRC check hardening tests.

#[cfg(test)]
#[allow(dead_code)]
pub mod support {
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{
        sign_drc_check_cancel_bound, sign_drc_check_cash_bound, sign_drc_check_create_bound,
        KeyPair,
    };
    use agora_types::{
        Amount, Block, BlockHeader, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, Hash,
        NativeAssetId, Transaction, TxOut, DRC_CHECK_CANCEL_TX_VERSION, DRC_CHECK_CASH_TX_VERSION,
        DRC_CHECK_CREATE_TX_VERSION,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_check::{check_owner_index_key, drc_check_root, lookup_drc_check_point};
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::supply::{
        load_burned_supply, load_issued_supply, put_burned_supply_into, put_issued_supply_into,
        put_schema_version_into,
    };
    use crate::{StateStore, SCHEMA_VERSION};

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

    #[allow(clippy::too_many_arguments)]
    pub fn signed_create(
        owner: &KeyPair,
        destination: agora_types::Address,
        amount: u64,
        expires_after_blue_score: Option<u64>,
        nonce: u64,
        ctx: &TxAuthContext,
        destination_tag: Option<u32>,
    ) -> DrcCheckCreateTx {
        let mut tx = DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TX_VERSION,
            owner: owner.address(),
            destination,
            amount: Amount::from_base_units(amount),
            fee: Amount::from_base_units(1),
            destination_tag,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_check_create_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_cash(
        submitter: &KeyPair,
        check_id: Hash,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcCheckCashTx {
        let mut tx = DrcCheckCashTx {
            version: DRC_CHECK_CASH_TX_VERSION,
            submitter: submitter.address(),
            check_id,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_check_cash_bound(&mut tx, submitter, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_cancel(
        submitter: &KeyPair,
        check_id: Hash,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcCheckCancelTx {
        let mut tx = DrcCheckCancelTx {
            version: DRC_CHECK_CANCEL_TX_VERSION,
            submitter: submitter.address(),
            check_id,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_check_cancel_bound(&mut tx, submitter, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn apply_check_block(
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

    #[allow(clippy::too_many_arguments)]
    pub fn create_live_check(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        amount: u64,
        expires_after_blue_score: Option<u64>,
        create_score: u64,
        ctx: &TxAuthContext,
    ) -> (Hash, DrcCheckCreateTx) {
        let nonce = load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let create = signed_create(
            owner,
            destination.address(),
            amount,
            expires_after_blue_score,
            nonce,
            ctx,
            None,
        );
        let id = create.check_id();
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_check_creates.push(create.clone());
        apply_check_block(store, block, create_score, ctx);
        assert_eq!(lookup_drc_check_point(store, &id).unwrap(), "live");
        (id, create)
    }

    pub struct CheckSnapshot {
        pub owner_balance: u64,
        pub destination_balance: u64,
        pub owner_nonce: u64,
        pub check_root: Hash,
        pub state_root: Hash,
        pub live_count: usize,
    }

    pub fn snapshot_check_state(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
    ) -> CheckSnapshot {
        CheckSnapshot {
            owner_balance: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance,
            destination_balance: load_account(store, NativeAssetId::DRC, &destination.address())
                .unwrap()
                .balance,
            owner_nonce: load_account(store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            check_root: drc_check_root(store).unwrap(),
            state_root: compose_trident_state_root(store, &TIP).unwrap(),
            live_count: count_live_checks(store, &owner.address()),
        }
    }

    pub fn count_live_checks(store: &StateStore, owner: &agora_types::Address) -> usize {
        use crate::columns::ColumnFamily;
        use borsh::BorshDeserialize;
        let key = check_owner_index_key(owner);
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

    pub fn locked_check_total(store: &StateStore, owner: &agora_types::Address) -> u64 {
        use crate::columns::ColumnFamily;
        use crate::drc_check::{check_owner_index_key, load_drc_check_live};
        use borsh::BorshDeserialize;
        let key = check_owner_index_key(owner);
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
                load_drc_check_live(store, id)
                    .unwrap()
                    .unwrap()
                    .amount
                    .as_base_units()
            })
            .sum()
    }

    pub fn assert_check_snapshot_unchanged(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        before: &CheckSnapshot,
    ) {
        let after = snapshot_check_state(store, owner, destination);
        assert_eq!(after.owner_balance, before.owner_balance);
        assert_eq!(after.destination_balance, before.destination_balance);
        assert_eq!(after.owner_nonce, before.owner_nonce);
        assert_eq!(after.check_root, before.check_root);
        assert_eq!(after.state_root, before.state_root);
        assert_eq!(after.live_count, before.live_count);
    }

    pub fn reject_check_apply_preserving_state(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        block: &Block,
        ctx: &TxAuthContext,
        before: &CheckSnapshot,
    ) {
        if agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
            .is_err()
        {
            assert_check_snapshot_unchanged(store, owner, destination, before);
            return;
        }
        assert!(
            apply_block_batched_with_auth_at_blue_score(store, block, 50, Some(ctx), 50).is_err()
        );
        assert_check_snapshot_unchanged(store, owner, destination, before);
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
            + locked_check_total(store, &owner.address())
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub mod multisign {
    use agora_crypto::{sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair};
    use agora_types::{
        materialize_drc_multisign_attachments, Amount, Block, DrcCheckCreateTx, DrcMultisignAuth,
        DrcMultisignEntry, DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId,
        DRC_CHECK_CREATE_TX_VERSION, DRC_MULTISIGN_AUTH_VERSION,
    };

    use super::support::{coinbase, signed_cancel, signed_cash, CheckSnapshot};
    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_check_test_harness::support::reject_check_apply_preserving_state;
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

    pub fn base_multisign_create_block(
        store: &StateStore,
        master: &KeyPair,
        destination: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, master, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &master.address())
            .unwrap()
            .nonce;
        let mut create = DrcCheckCreateTx {
            version: DRC_CHECK_CREATE_TX_VERSION,
            owner: master.address(),
            destination: destination.address(),
            amount: Amount::from_base_units(10),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            expires_after_blue_score: Some(100),
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
        block.drc_check_creates.push(create);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn base_multisign_cash_block(
        store: &StateStore,
        _owner: &KeyPair,
        destination: &KeyPair,
        check_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, destination, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &destination.address())
            .unwrap()
            .nonce;
        let mut cash = signed_cash(destination, check_id, nonce, ctx);
        cash.public_key.clear();
        cash.signature.clear();
        cash.multisign = Some(multisign_bundle(
            destination.address(),
            &cash.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], destination);
        block.drc_check_cashes.push(cash);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn multisign_cash_block(
        store: &StateStore,
        submitter: &KeyPair,
        check_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        let nonce = load_account(store, NativeAssetId::DRC, &submitter.address())
            .unwrap()
            .nonce;
        let mut cash = signed_cash(submitter, check_id, nonce, ctx);
        cash.public_key.clear();
        cash.signature.clear();
        cash.multisign = Some(multisign_bundle(
            submitter.address(),
            &cash.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], submitter);
        block.drc_check_cashes.push(cash);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn multisign_cancel_block(
        store: &StateStore,
        submitter: &KeyPair,
        coinbase_payout: &KeyPair,
        check_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        let nonce = load_account(store, NativeAssetId::DRC, &submitter.address())
            .unwrap()
            .nonce;
        let mut cancel = signed_cancel(submitter, check_id, nonce, ctx);
        cancel.public_key.clear();
        cancel.signature.clear();
        cancel.multisign = Some(multisign_bundle(
            submitter.address(),
            &cancel.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], coinbase_payout);
        block.drc_check_cancels.push(cancel);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn base_multisign_cancel_block(
        store: &StateStore,
        submitter: &KeyPair,
        owner: &KeyPair,
        _destination: &KeyPair,
        check_id: Hash,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, submitter, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &submitter.address())
            .unwrap()
            .nonce;
        let mut cancel = signed_cancel(submitter, check_id, nonce, ctx);
        cancel.public_key.clear();
        cancel.signature.clear();
        cancel.multisign = Some(multisign_bundle(
            submitter.address(),
            &cancel.signing_bytes_bound(&ctx.chain_id, &ctx.genesis),
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], owner);
        block.drc_check_cancels.push(cancel);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block
    }

    pub fn reject_preserving(
        store: &StateStore,
        owner: &KeyPair,
        destination: &KeyPair,
        mut block: Block,
        ctx: &TxAuthContext,
        before: &CheckSnapshot,
    ) {
        block.header.tx_root = block.compute_body_root();
        reject_check_apply_preserving_state(store, owner, destination, &block, ctx, before);
    }
}
