//! Shared helpers for issuer-scoped trust line / issued-transfer tests.

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub mod support {
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{sign_drc_issued_transfer_bound, sign_drc_trust_line_set_bound, KeyPair};
    use agora_types::{
        Amount, Block, BlockHeader, DrcIssuedTransferTx, DrcTrustLineSetTx, Hash, IssuedAmount,
        IssuedAssetId, IssuedCurrencyCode, NativeAssetId, Transaction, TxOut,
        DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
    };

    use crate::accounts::credit_account_into;
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_trust_line::{
        drc_trust_line_root, load_drc_issued_transfer_receipt, load_drc_issuer_liability,
        load_drc_trust_line_live,
    };
    use crate::store::WriteBatch;
    use crate::supply::{
        load_burned_supply, load_issued_supply, put_burned_supply_into, put_issued_supply_into,
        put_schema_version_into,
    };
    use crate::{StateStore, SCHEMA_VERSION};

    static COINBASE_SEQ: AtomicU64 = AtomicU64::new(8_000_000);

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
        put_burned_supply_into(
            &mut batch,
            NativeAssetId::DRC,
            load_burned_supply(store, NativeAssetId::DRC).unwrap(),
        );
        put_schema_version_into(&mut batch, SCHEMA_VERSION);
        store.write_batch(batch).unwrap();
    }

    pub fn std_code(tag: &[u8; 3]) -> IssuedCurrencyCode {
        let mut c = [0u8; 20];
        c[..3].copy_from_slice(tag);
        IssuedCurrencyCode(c)
    }

    pub fn asset(issuer: &KeyPair, currency: IssuedCurrencyCode) -> IssuedAssetId {
        IssuedAssetId {
            issuer: issuer.address(),
            currency,
        }
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

    pub fn signed_trust_line_set(
        holder: &KeyPair,
        issuer: &KeyPair,
        currency: IssuedCurrencyCode,
        limit: u64,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcTrustLineSetTx {
        let mut tx = DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency,
            limit: IssuedAmount::from_units(limit),
            fee: Amount::from_base_units(fee),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_trust_line_set_bound(&mut tx, holder, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn signed_issued_transfer(
        sender: &KeyPair,
        recipient: agora_types::Address,
        issuer: &KeyPair,
        currency: IssuedCurrencyCode,
        amount: u64,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcIssuedTransferTx {
        let mut tx = DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: sender.address(),
            recipient,
            issuer: issuer.address(),
            currency,
            amount: IssuedAmount::from_units(amount),
            fee: Amount::from_base_units(fee),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_transfer_bound(&mut tx, sender, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn apply_block(store: &StateStore, block: Block, blue: u64, ctx: &TxAuthContext) {
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue)
                .unwrap();
        store.write_batch(result.batch).unwrap();
    }

    pub fn line_balance(
        store: &StateStore,
        holder: agora_types::Address,
        asset: &IssuedAssetId,
    ) -> u64 {
        load_drc_trust_line_live(store, &holder, asset)
            .unwrap()
            .map(|l| l.balance.as_units())
            .unwrap_or(0)
    }

    pub fn issuer_outstanding(store: &StateStore, asset: &IssuedAssetId) -> u64 {
        load_drc_issuer_liability(store, asset).unwrap().as_units()
    }

    pub fn receipt_exists(store: &StateStore, tx_id: &Hash) -> bool {
        load_drc_issued_transfer_receipt(store, tx_id)
            .unwrap()
            .is_some()
    }

    pub fn trust_root(store: &StateStore) -> Hash {
        drc_trust_line_root(store).unwrap()
    }

    pub fn apply_block_journal(
        store: &StateStore,
        block: Block,
        blue: u64,
        ctx: &TxAuthContext,
    ) -> crate::apply::UtxoJournal {
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &block, 50, Some(ctx), blue)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        result.journal
    }

    pub fn revert_journal(store: &StateStore, journal: &crate::apply::UtxoJournal) {
        crate::apply::revert_journal(store, journal).unwrap();
    }

    pub const TIP: Hash = Hash([4; 32]);

    pub fn mint_ticket(store: &StateStore, owner: &KeyPair) -> u64 {
        use agora_crypto::sign_drc_ticket_create_bound;
        use agora_types::DrcTicketCreateTx;
        let ctx = auth();
        let nonce = crate::accounts::load_account(store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let mut create = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, nonce);
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

    pub fn setup_live_line(
        store: &StateStore,
        holder: &KeyPair,
        issuer: &KeyPair,
        cur: IssuedCurrencyCode,
        limit: u64,
        blue: u64,
    ) {
        let ctx = auth();
        fund(store, holder, 500);
        fund(store, issuer, 500);
        let mut block = coinbase(vec![Hash::ZERO], holder);
        block.drc_trust_line_sets.push(signed_trust_line_set(
            holder, issuer, cur, limit, 1, 0, &ctx,
        ));
        block.header.tx_root = block.compute_body_root();
        apply_block(store, block, blue, &ctx);
    }
}

#[cfg(test)]
pub mod invariants {
    use super::support::{asset, issuer_outstanding, line_balance, trust_root, TIP};
    use crate::accounts::load_account;
    use crate::drc_ticket::load_drc_account_tickets;
    use crate::drc_trust_line::{
        count_live_trust_line_holders_for_issuer, count_live_trust_lines_for_holder,
    };
    use crate::state_root::compose_trident_state_root;
    use crate::StateStore;
    use agora_crypto::KeyPair;
    use agora_types::{Address, Hash, IssuedCurrencyCode, NativeAssetId};

    #[derive(Clone, Debug)]
    pub struct TrustLineInvariantSnap {
        pub trust_root: Hash,
        pub state_root: Hash,
        pub liability: u64,
        pub holder_lines: Vec<(Address, u64)>,
        pub native_balances: Vec<(Address, u64)>,
        pub nonces: Vec<(Address, u64)>,
        pub tickets: Vec<(Address, Vec<u64>)>,
        pub holder_line_counts: Vec<(Address, usize)>,
        pub issuer_holder_count: usize,
    }

    pub fn snapshot(
        store: &StateStore,
        issuer: &KeyPair,
        cur: IssuedCurrencyCode,
        holders: &[&KeyPair],
        ticket_owners: &[&KeyPair],
    ) -> TrustLineInvariantSnap {
        let ast = asset(issuer, cur);
        let mut native = std::collections::BTreeMap::new();
        let mut nonce_map = std::collections::BTreeMap::new();
        for kp in holders
            .iter()
            .copied()
            .chain(std::iter::once(issuer))
            .chain(ticket_owners.iter().copied())
        {
            let acct = load_account(store, NativeAssetId::DRC, &kp.address()).unwrap();
            native.insert(kp.address(), acct.balance);
            nonce_map.insert(kp.address(), acct.nonce);
        }
        TrustLineInvariantSnap {
            trust_root: trust_root(store),
            state_root: compose_trident_state_root(store, &TIP).unwrap(),
            liability: issuer_outstanding(store, &ast),
            holder_lines: holders
                .iter()
                .map(|h| (h.address(), line_balance(store, h.address(), &ast)))
                .collect(),
            native_balances: native.into_iter().collect(),
            nonces: nonce_map.into_iter().collect(),
            tickets: ticket_owners
                .iter()
                .map(|kp| {
                    (
                        kp.address(),
                        load_drc_account_tickets(store, &kp.address()).unwrap(),
                    )
                })
                .collect(),
            holder_line_counts: holders
                .iter()
                .map(|h| {
                    (
                        h.address(),
                        count_live_trust_lines_for_holder(store, &h.address()).unwrap(),
                    )
                })
                .collect(),
            issuer_holder_count: count_live_trust_line_holders_for_issuer(
                store,
                &issuer.address(),
                &cur,
            )
            .unwrap(),
        }
    }

    pub fn assert_unchanged(
        store: &StateStore,
        issuer: &KeyPair,
        cur: IssuedCurrencyCode,
        holders: &[&KeyPair],
        ticket_owners: &[&KeyPair],
        before: &TrustLineInvariantSnap,
    ) {
        let after = snapshot(store, issuer, cur, holders, ticket_owners);
        assert_eq!(after.trust_root, before.trust_root);
        assert_eq!(after.state_root, before.state_root);
        assert_eq!(after.liability, before.liability);
        assert_eq!(after.holder_lines, before.holder_lines);
        assert_eq!(after.native_balances, before.native_balances);
        assert_eq!(after.nonces, before.nonces);
        assert_eq!(after.tickets, before.tickets);
        assert_eq!(after.holder_line_counts, before.holder_line_counts);
        assert_eq!(after.issuer_holder_count, before.issuer_holder_count);
    }

    pub fn reject_block_preserving(
        store: &StateStore,
        block: &agora_types::Block,
        ctx: &crate::apply::TxAuthContext,
        issuer: &KeyPair,
        cur: IssuedCurrencyCode,
        holders: &[&KeyPair],
        ticket_owners: &[&KeyPair],
    ) {
        let before = snapshot(store, issuer, cur, holders, ticket_owners);
        if agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
            .is_err()
        {
            assert_unchanged(store, issuer, cur, holders, ticket_owners, &before);
            return;
        }
        assert!(crate::apply::apply_block_batched_with_auth_at_blue_score(
            store,
            block,
            50,
            Some(ctx),
            50
        )
        .is_err());
        assert_unchanged(store, issuer, cur, holders, ticket_owners, &before);
    }
}

#[cfg(test)]
pub mod policy {
    use agora_crypto::sign_drc_account_policy_bound;
    use agora_types::{Amount, DrcAccountPolicyTx, DrcDepositPreauthTx};

    use crate::apply::TxAuthContext;
    use crate::drc_deposit_preauth::apply_drc_deposit_preauth;
    use crate::drc_policy::apply_drc_account_policy;
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};
    use agora_crypto::KeyPair;

    pub fn set_require_dest_tag(store: &StateStore, account: &KeyPair, ctx: &TxAuthContext) {
        let nonce = crate::accounts::load_account(
            store,
            agora_types::NativeAssetId::DRC,
            &account.address(),
        )
        .unwrap()
        .nonce;
        let mut tx =
            DrcAccountPolicyTx::set_require_destination_tag(account.address(), Amount::ZERO, nonce);
        sign_drc_account_policy_bound(&mut tx, account, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    pub fn clear_require_dest_tag(store: &StateStore, account: &KeyPair, ctx: &TxAuthContext) {
        let nonce = crate::accounts::load_account(
            store,
            agora_types::NativeAssetId::DRC,
            &account.address(),
        )
        .unwrap()
        .nonce;
        let mut tx = DrcAccountPolicyTx::clear_require_destination_tag(
            account.address(),
            Amount::ZERO,
            nonce,
        );
        sign_drc_account_policy_bound(&mut tx, account, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    pub fn set_deposit_auth(store: &StateStore, recipient: &KeyPair, ctx: &TxAuthContext) {
        let nonce = crate::accounts::load_account(
            store,
            agora_types::NativeAssetId::DRC,
            &recipient.address(),
        )
        .unwrap()
        .nonce;
        let mut tx =
            DrcAccountPolicyTx::set_deposit_auth_required(recipient.address(), Amount::ZERO, nonce);
        sign_drc_account_policy_bound(&mut tx, recipient, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_account_policy(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    pub fn grant_deposit_preauth(
        store: &StateStore,
        recipient: &KeyPair,
        source: agora_types::Address,
        ctx: &TxAuthContext,
    ) {
        let nonce = crate::accounts::load_account(
            store,
            agora_types::NativeAssetId::DRC,
            &recipient.address(),
        )
        .unwrap()
        .nonce;
        let mut tx =
            DrcDepositPreauthTx::authorize(recipient.address(), source, Amount::ZERO, nonce);
        agora_crypto::sign_drc_deposit_preauth_bound(
            &mut tx,
            recipient,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_deposit_preauth(store, &tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub mod multisign {
    use agora_crypto::{sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair};
    use agora_types::{
        materialize_drc_multisign_attachments, Amount, Block, DrcMultisignAuth, DrcMultisignEntry,
        DrcSignerListEntry, DrcSignerListTx, DrcTrustLineSetTx, NativeAssetId,
        DRC_MULTISIGN_AUTH_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
    };

    use super::support::{coinbase, std_code};
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

    pub fn base_multisign_trust_line_set_block(
        store: &StateStore,
        holder: &KeyPair,
        issuer: &KeyPair,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, holder, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &holder.address())
            .unwrap()
            .nonce;
        let mut tx = DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency: std_code(b"USD"),
            limit: agora_types::IssuedAmount::from_units(100),
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            holder.address(),
            &signing_bytes,
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![agora_types::Hash::ZERO], holder);
        block.drc_trust_line_sets.push(tx);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        block
    }

    pub fn base_multisign_issued_transfer_block(
        store: &StateStore,
        sender: &KeyPair,
        recipient: agora_types::Address,
        issuer: &KeyPair,
        currency: agora_types::IssuedCurrencyCode,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        use agora_types::DrcIssuedTransferTx;
        use agora_types::DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION;

        install_signer_list(store, sender, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &sender.address())
            .unwrap()
            .nonce;
        let mut tx = DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: sender.address(),
            recipient,
            issuer: issuer.address(),
            currency,
            amount: agora_types::IssuedAmount::from_units(5),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: agora_types::Hash::ZERO,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            sender.address(),
            &signing_bytes,
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![agora_types::Hash::ZERO], sender);
        block.drc_issued_transfers.push(tx);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        block
    }
}
