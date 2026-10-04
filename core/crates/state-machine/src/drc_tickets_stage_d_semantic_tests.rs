//! Stage-D: per-family semantic rejection after sequence resolution (valid auth).

#[cfg(test)]
mod semantic_seven_families {
    use agora_crypto::{
        sign_account_transfer_bound, sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound,
        sign_drc_payment_bound, sign_drc_regular_key_bound, sign_drc_signer_list_bound,
        sign_stake_tx_bound, KeyPair,
    };
    use agora_types::{
        AccountTransfer, Amount, DrcAccountPolicyTx, DrcAccountSequenceSelector,
        DrcDepositPreauthTx, DrcPaymentTx, DrcRegularKeyTx, DrcSignerListEntry, DrcSignerListTx,
        Hash, NativeAssetId, SignedStakeTx, StakeOpKind, ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
        DRC_ACCOUNT_POLICY_TICKET_TX_VERSION, DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION,
        DRC_PAYMENT_TICKET_VERSION, DRC_REGULAR_KEY_TICKET_TX_VERSION,
        DRC_SIGNER_LIST_TICKET_TX_VERSION, STAKE_TX_TICKET_VERSION,
    };

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_regular_key::apply_drc_regular_key;
    use crate::drc_tickets_test_harness::support::{
        auth, coinbase, fund, key, mint_ticket, reject_block_preserving_ticket_state,
        snapshot_ticket_state,
    };
    use crate::store::WriteBatch;
    use crate::{AccountJournal, StateStore};

    fn apply_regular_key(store: &StateStore, tx: &DrcRegularKeyTx, ctx: &TxAuthContext) {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_regular_key(store, tx, ctx, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn semantic_transfer_insufficient_balance_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(1);
        fund(&store, &owner, 5);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut transfer = AccountTransfer {
            version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            from: owner.address(),
            to: key(2).address(),
            amount: Amount::from_base_units(1_000),
            fee: Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.account_transfers.push(transfer);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn semantic_stake_below_minimum_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(3);
        fund(&store, &owner, 50_000_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut stake = SignedStakeTx {
            version: STAKE_TX_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            kind: StakeOpKind::Bond,
            actor: owner.address(),
            validator: owner.address(),
            amount: 1,
            consensus_pubkey: vec![2; 33],
            withdrawal: owner.address(),
            commission_bps: 100,
            metadata_hash: Hash::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_stake_tx_bound(&mut stake, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.stake_ops.push(stake);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn semantic_regular_key_clear_lockout_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let master = key(4);
        let regular = key(5);
        fund(&store, &master, 500_000);
        let ctx = auth();
        let mut set = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        sign_drc_regular_key_bound(&mut set, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        apply_regular_key(&store, &set, &ctx);
        let seq = mint_ticket(&store, &master, &ctx, vec![Hash::ZERO]).0;
        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::ZERO,
            load_account(&store, NativeAssetId::DRC, &master.address())
                .unwrap()
                .nonce,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut disable_block = coinbase(vec![Hash::ZERO], &master);
        disable_block.drc_account_policies.push(disable);
        disable_block.header.tx_root = disable_block.compute_body_root();
        store
            .write_batch(
                apply_block_batched_with_auth_at_blue_score(
                    &store,
                    &disable_block,
                    50,
                    Some(&ctx),
                    50,
                )
                .unwrap()
                .batch,
            )
            .unwrap();
        let before = snapshot_ticket_state(&store, &master);
        let mut clear = DrcRegularKeyTx::clear(master.address(), Amount::ZERO, 0);
        clear.version = DRC_REGULAR_KEY_TICKET_TX_VERSION;
        clear.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_regular_key_bound(&mut clear, &regular, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block2 = coinbase(vec![disable_block.id()], &master);
        block2.drc_regular_keys.push(clear);
        block2.header.tx_root = block2.compute_body_root();
        reject_block_preserving_ticket_state(&store, &master, &block2, &ctx, &before);
    }

    #[test]
    fn semantic_signer_list_insufficient_fee_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(6);
        fund(&store, &owner, 100);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut sl = DrcSignerListTx::unsigned_set(
            owner.address(),
            agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
                signer: key(7).address(),
                weight: 1,
            }]),
            1,
            Amount::from_base_units(1_000),
            0,
        );
        sl.version = DRC_SIGNER_LIST_TICKET_TX_VERSION;
        sl.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_signer_list_bound(&mut sl, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_signer_lists.push(sl);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn semantic_policy_disable_master_without_recovery_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(8);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut disable =
            DrcAccountPolicyTx::set_master_key_disabled(owner.address(), Amount::ZERO, 0);
        disable.version = DRC_ACCOUNT_POLICY_TICKET_TX_VERSION;
        disable.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_account_policy_bound(&mut disable, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_account_policies.push(disable);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn semantic_deposit_preauth_missing_revoke_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(9);
        let source = key(10);
        fund(&store, &owner, 100_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before = snapshot_ticket_state(&store, &owner);
        let mut revoke =
            DrcDepositPreauthTx::unauthorize(owner.address(), source.address(), Amount::ZERO, 0);
        revoke.version = DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION;
        revoke.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_deposit_preauth_bound(&mut revoke, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_deposit_preauths.push(revoke);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &owner, &block, &ctx, &before);
    }

    #[test]
    fn semantic_payment_missing_required_destination_tag_preserves_ticket() {
        let store = StateStore::open_in_memory();
        let payer = key(11);
        let payee = key(12);
        fund(&store, &payer, 500_000);
        fund(&store, &payee, 10_000);
        let ctx = auth();
        let mut policy =
            DrcAccountPolicyTx::set_require_destination_tag(payee.address(), Amount::ZERO, 0);
        sign_drc_account_policy_bound(&mut policy, &payee, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut policy_block = coinbase(vec![Hash::ZERO], &payer);
        policy_block.drc_account_policies.push(policy);
        policy_block.header.tx_root = policy_block.compute_body_root();
        store
            .write_batch(
                apply_block_batched_with_auth_at_blue_score(
                    &store,
                    &policy_block,
                    50,
                    Some(&ctx),
                    50,
                )
                .unwrap()
                .batch,
            )
            .unwrap();
        let seq = mint_ticket(&store, &payer, &ctx, vec![policy_block.id()]).0;
        let before = snapshot_ticket_state(&store, &payer);
        let mut payment = DrcPaymentTx::unsigned_v4(
            payer.address(),
            payee.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut payment, &payer, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![policy_block.id()], &payer);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        reject_block_preserving_ticket_state(&store, &payer, &block, &ctx, &before);
    }
}
