//! Stage-C adversarial matrix: Borsh transport, rejection atomicity, duplicate consumers, reorg/RocksDB.

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    use agora_crypto::{
        sign_account_transfer_bound, sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound,
        sign_drc_payment_bound, sign_drc_regular_key_bound, sign_drc_signer_list_bound,
        sign_drc_ticket_create_bound, sign_stake_tx_bound, KeyPair,
    };
    use agora_types::{
        AccountTransfer, Amount, Block, BlockHeader, DrcAccountPolicyTx,
        DrcAccountSequenceSelector, DrcDepositPreauthTx, DrcPaymentTx, DrcRegularKeyTx,
        DrcSignerListEntry, DrcSignerListTx, DrcTicketCreateTx, Hash, NativeAssetId, SignedStakeTx,
        StakeOpKind, Transaction, TransactionAcceptance, TxOut,
        ACCOUNT_TRANSFER_DRC_TICKET_VERSION, DRC_ACCOUNT_POLICY_TICKET_TX_VERSION,
        DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION, DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT,
        DRC_PAYMENT_TICKET_VERSION, DRC_REGULAR_KEY_TICKET_TX_VERSION,
        DRC_SIGNER_LIST_TICKET_TX_VERSION, STAKE_TX_TICKET_VERSION,
    };
    use borsh::BorshDeserialize;

    use crate::accounts::{credit_account_into, load_account};
    use crate::apply::{
        apply_block_batched_virtual_at_blue_score, apply_block_batched_with_auth_at_blue_score,
        revert_journal_batched, TxAuthContext,
    };
    use crate::drc_ticket::{drc_ticket_root, load_drc_account_tickets};
    use crate::state_root::compose_trident_state_root;
    use crate::store::WriteBatch;
    use crate::supply::{
        load_burned_supply, load_issued_supply, put_burned_supply_into, put_issued_supply_into,
        put_schema_version_into,
    };
    use crate::{StateStore, SCHEMA_VERSION};

    const TIP: Hash = Hash([4; 32]);

    static COINBASE_SEQ: AtomicU64 = AtomicU64::new(1_000_000);

    fn next_coinbase_seq() -> u64 {
        COINBASE_SEQ.fetch_add(1, Ordering::Relaxed)
    }

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

    fn coinbase(parents: Vec<Hash>, payout: &KeyPair) -> Block {
        let seq = next_coinbase_seq();
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

    fn signed_create(owner: &KeyPair, nonce: u64, ctx: &TxAuthContext) -> DrcTicketCreateTx {
        let mut tx = DrcTicketCreateTx::unsigned(owner.address(), Amount::ZERO, nonce);
        sign_drc_ticket_create_bound(&mut tx, owner, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    fn mint_ticket(
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
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.header.tx_root, block.header.tx_root);
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &decoded, 50, Some(ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(load_drc_account_tickets(store, &owner.address())
            .unwrap()
            .contains(&seq));
        (seq, block.id())
    }

    fn apply_borsh_roundtrip_apply_once(store: &StateStore, block: &Block, ctx: &TxAuthContext) {
        let bytes = borsh::to_vec(block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.compute_body_root(), block.header.tx_root);
        let result =
            apply_block_batched_with_auth_at_blue_score(store, &decoded, 50, Some(ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
    }

    #[test]
    fn seven_families_ticket_spend_borsh_roundtrip_harness() {
        let store = StateStore::open_in_memory();
        let owner = key(20);
        let peer = key(21);
        fund(&store, &owner, 50_000_000);
        let ctx = auth();
        let mut parents = vec![Hash::ZERO];

        // Account transfer v3 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut transfer = AccountTransfer {
            version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            from: owner.address(),
            to: peer.address(),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(parents.clone(), &owner);
        block.account_transfers.push(transfer);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
        parents = vec![block.id()];

        // Stake bond v2 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut stake = SignedStakeTx {
            version: STAKE_TX_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            kind: StakeOpKind::Bond,
            actor: owner.address(),
            validator: owner.address(),
            amount: 1_000_000,
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
        let mut block = coinbase(parents.clone(), &owner);
        block.stake_ops.push(stake);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
        parents = vec![block.id()];

        // Regular key v2 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut rk = DrcRegularKeyTx::set(
            owner.address(),
            peer.address(),
            peer.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        rk.version = DRC_REGULAR_KEY_TICKET_TX_VERSION;
        rk.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_regular_key_bound(&mut rk, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_regular_keys.push(rk);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
        parents = vec![block.id()];

        // Signer list v2 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut sl = DrcSignerListTx::unsigned_set(
            owner.address(),
            agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
                signer: peer.address(),
                weight: 1,
            }]),
            1,
            Amount::ZERO,
            0,
        );
        sl.version = DRC_SIGNER_LIST_TICKET_TX_VERSION;
        sl.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_signer_list_bound(&mut sl, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_signer_lists.push(sl);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
        parents = vec![block.id()];

        // Policy v4 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut policy =
            DrcAccountPolicyTx::set_require_destination_tag(owner.address(), Amount::ZERO, 0);
        policy.version = DRC_ACCOUNT_POLICY_TICKET_TX_VERSION;
        policy.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_account_policy_bound(&mut policy, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_account_policies.push(policy);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
        parents = vec![block.id()];

        // Deposit preauth v2 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut pre =
            DrcDepositPreauthTx::authorize(owner.address(), peer.address(), Amount::ZERO, 0);
        pre.version = DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION;
        pre.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_deposit_preauth_bound(&mut pre, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_deposit_preauths.push(pre);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
        parents = vec![block.id()];

        // Payment v5 + ticket
        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
            Amount::from_base_units(2),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(parents, &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        apply_borsh_roundtrip_apply_once(&store, &block, &ctx);
    }

    #[test]
    fn duplicate_ticket_consumer_first_lane_wins_deterministic() {
        let store = StateStore::open_in_memory();
        let owner = key(30);
        let peer = key(31);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;

        let mut transfer = AccountTransfer {
            version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            from: owner.address(),
            to: peer.address(),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis).unwrap();

        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
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
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();

        let state_root_before = compose_trident_state_root(&store, &TIP).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.account_transfers.push(transfer);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        let result =
            apply_block_batched_virtual_at_blue_score(&store, &decoded, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            result.acceptance.account_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert_eq!(
            result.acceptance.payment_statuses,
            vec![TransactionAcceptance::ConflictLost]
        );
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .is_empty());
        assert_ne!(
            compose_trident_state_root(&store, &TIP).unwrap(),
            state_root_before
        );
    }

    #[test]
    fn semantic_rejection_preserves_ticket_nonce_and_roots() {
        let store = StateStore::open_in_memory();
        let owner = key(40);
        fund(&store, &owner, 100);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let before_nonce = load_account(&store, NativeAssetId::DRC, &owner.address())
            .unwrap()
            .nonce;
        let tickets_before = load_drc_account_tickets(&store, &owner.address()).unwrap();
        let ticket_root = drc_ticket_root(&store).unwrap();
        let state_root = compose_trident_state_root(&store, &TIP).unwrap();

        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(41).address(),
            Amount::from_base_units(10_000),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        assert!(
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .is_err()
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce,
            before_nonce
        );
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            tickets_before
        );
        assert_eq!(drc_ticket_root(&store).unwrap(), ticket_root);
        assert_eq!(
            compose_trident_state_root(&store, &TIP).unwrap(),
            state_root
        );
    }

    #[test]
    fn seven_families_invalid_auth_preserves_ticket_and_roots() {
        let store = StateStore::open_in_memory();
        let owner = key(45);
        let peer = key(46);
        fund(&store, &owner, 50_000_000);
        let ctx = auth();
        let mut parents = vec![Hash::ZERO];

        macro_rules! assert_ticket_preserved_after_bad_auth {
            ($block:expr) => {{
                let tickets_before = load_drc_account_tickets(&store, &owner.address()).unwrap();
                let ticket_root = drc_ticket_root(&store).unwrap();
                let state_root = compose_trident_state_root(&store, &TIP).unwrap();
                let nonce_before = load_account(&store, NativeAssetId::DRC, &owner.address())
                    .unwrap()
                    .nonce;
                assert!(apply_block_batched_with_auth_at_blue_score(
                    &store,
                    &$block,
                    50,
                    Some(&ctx),
                    50
                )
                .is_err());
                assert_eq!(
                    load_drc_account_tickets(&store, &owner.address()).unwrap(),
                    tickets_before
                );
                assert_eq!(
                    load_account(&store, NativeAssetId::DRC, &owner.address())
                        .unwrap()
                        .nonce,
                    nonce_before
                );
                assert_eq!(drc_ticket_root(&store).unwrap(), ticket_root);
                assert_eq!(
                    compose_trident_state_root(&store, &TIP).unwrap(),
                    state_root
                );
            }};
        }

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut transfer = AccountTransfer {
            version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            from: owner.address(),
            to: peer.address(),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        transfer.signature[0] ^= 0xff;
        let mut block = coinbase(parents.clone(), &owner);
        block.account_transfers.push(transfer);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
        parents = vec![block.id()];

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut stake = SignedStakeTx {
            version: STAKE_TX_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            kind: StakeOpKind::Bond,
            actor: owner.address(),
            validator: owner.address(),
            amount: 1_000_000,
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
        stake.signature[0] ^= 0xff;
        let mut block = coinbase(parents.clone(), &owner);
        block.stake_ops.push(stake);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
        parents = vec![block.id()];

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut rk = DrcRegularKeyTx::set(
            owner.address(),
            peer.address(),
            peer.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        rk.version = DRC_REGULAR_KEY_TICKET_TX_VERSION;
        rk.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_regular_key_bound(&mut rk, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        rk.signature[0] ^= 0xff;
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_regular_keys.push(rk);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
        parents = vec![block.id()];

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut sl = DrcSignerListTx::unsigned_set(
            owner.address(),
            agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
                signer: peer.address(),
                weight: 1,
            }]),
            1,
            Amount::ZERO,
            0,
        );
        sl.version = DRC_SIGNER_LIST_TICKET_TX_VERSION;
        sl.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_signer_list_bound(&mut sl, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        sl.signature[0] ^= 0xff;
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_signer_lists.push(sl);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
        parents = vec![block.id()];

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut policy =
            DrcAccountPolicyTx::set_require_destination_tag(owner.address(), Amount::ZERO, 0);
        policy.version = DRC_ACCOUNT_POLICY_TICKET_TX_VERSION;
        policy.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_account_policy_bound(&mut policy, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        policy.signature[0] ^= 0xff;
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_account_policies.push(policy);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
        parents = vec![block.id()];

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut pre =
            DrcDepositPreauthTx::authorize(owner.address(), peer.address(), Amount::ZERO, 0);
        pre.version = DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION;
        pre.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
        sign_drc_deposit_preauth_bound(&mut pre, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        pre.signature[0] ^= 0xff;
        let mut block = coinbase(parents.clone(), &owner);
        block.drc_deposit_preauths.push(pre);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
        parents = vec![block.id()];

        let seq = mint_ticket(&store, &owner, &ctx, parents.clone()).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
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
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        payment.signature[0] ^= 0xff;
        let mut block = coinbase(parents, &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        assert_ticket_preserved_after_bad_auth!(block);
    }

    #[test]
    fn disabled_master_ticket_payment_rejected_ticket_stays_live() {
        let store = StateStore::open_in_memory();
        let master = key(50);
        let regular = key(51);
        let payee = key(52);
        fund(&store, &master, 2_000_000);
        let ctx = auth();

        let mut set_rk = DrcRegularKeyTx::set(
            master.address(),
            regular.address(),
            regular.public_key_bytes().to_vec(),
            Amount::ZERO,
            0,
        );
        sign_drc_regular_key_bound(&mut set_rk, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &master);
        block.drc_regular_keys.push(set_rk);
        block.header.tx_root = block.compute_body_root();
        store
            .write_batch(
                apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                    .unwrap()
                    .batch,
            )
            .unwrap();

        let seq = mint_ticket(&store, &master, &ctx, vec![block.id()]).0;

        let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
            master.address(),
            Amount::ZERO,
            load_account(&store, NativeAssetId::DRC, &master.address())
                .unwrap()
                .nonce,
        );
        sign_drc_account_policy_bound(&mut disable, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block2 = coinbase(vec![block.id()], &master);
        block2.drc_account_policies.push(disable);
        block2.header.tx_root = block2.compute_body_root();
        store
            .write_batch(
                apply_block_batched_with_auth_at_blue_score(&store, &block2, 50, Some(&ctx), 50)
                    .unwrap()
                    .batch,
            )
            .unwrap();

        assert_eq!(
            load_drc_account_tickets(&store, &master.address()).unwrap(),
            vec![seq]
        );

        let mut payment = DrcPaymentTx::unsigned_v4(
            master.address(),
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
        sign_drc_payment_bound(&mut payment, &master, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut pay_block = coinbase(vec![block2.id()], &master);
        pay_block.drc_payments.push(payment);
        pay_block.header.tx_root = pay_block.compute_body_root();
        assert!(apply_block_batched_with_auth_at_blue_score(
            &store,
            &pay_block,
            50,
            Some(&ctx),
            50
        )
        .is_err());
        assert_eq!(
            load_drc_account_tickets(&store, &master.address()).unwrap(),
            vec![seq]
        );
    }

    #[test]
    fn selector_tamper_breaks_single_sign_auth() {
        let store = StateStore::open_in_memory();
        let owner = key(60);
        fund(&store, &owner, 500);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(61).address(),
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
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq + 1));
        let mut batch = WriteBatch::new();
        let mut journal = crate::AccountJournal::default();
        assert!(crate::payments::apply_drc_payment(
            &store,
            &payment,
            &ctx,
            &mut batch,
            &mut journal
        )
        .is_err());
    }

    #[test]
    fn ticket_cap_and_overflow_rejected() {
        let store = StateStore::open_in_memory();
        let owner = key(70);
        fund(&store, &owner, 100_000_000);
        let ctx = auth();
        let mut parents = vec![Hash::ZERO];
        let mut nonce = 0u64;
        for _ in 0..DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT {
            let create = signed_create(&owner, nonce, &ctx);
            let mut block = coinbase(parents.clone(), &owner);
            block.drc_ticket_creates.push(create);
            block.header.tx_root = block.compute_body_root();
            store
                .write_batch(
                    apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                        .unwrap()
                        .batch,
                )
                .unwrap();
            parents = vec![block.id()];
            nonce = load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce;
        }
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address())
                .unwrap()
                .len(),
            DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT
        );
        let overflow_create = signed_create(&owner, nonce, &ctx);
        let mut block = coinbase(parents, &owner);
        block.drc_ticket_creates.push(overflow_create);
        block.header.tx_root = block.compute_body_root();
        assert!(
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .is_err()
        );
    }

    #[test]
    fn reorg_journal_restores_ticket_after_spend() {
        let store = StateStore::open_in_memory();
        let owner = key(80);
        let peer = key(81);
        fund(&store, &owner, 500_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
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
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        let result =
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(load_drc_account_tickets(&store, &owner.address())
            .unwrap()
            .is_empty());
        store
            .write_batch(revert_journal_batched(&result.journal).unwrap())
            .unwrap();
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![seq]
        );
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_reopen_preserves_ticket_set_and_auth() {
        let dir = std::env::temp_dir().join(format!(
            "agora-drc-tickets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let owner = key(90);
        let ctx = auth();
        {
            let store = StateStore::open(&dir).unwrap();
            fund(&store, &owner, 100_000);
            let _ = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]);
        }
        let store = StateStore::open(&dir).unwrap();
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![1]
        );
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            key(91).address(),
            Amount::from_base_units(200_000_000),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
            None,
        );
        payment.version = DRC_PAYMENT_TICKET_VERSION;
        payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(1));
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        assert!(apply_block_batched_with_auth_at_blue_score(
            &store,
            &{
                let mut block = coinbase(vec![Hash::ZERO], &owner);
                block.drc_payments.push(payment);
                block.header.tx_root = block.compute_body_root();
                block
            },
            50,
            Some(&ctx),
            50
        )
        .is_err());
    }

    #[test]
    fn cross_owner_ticket_spend_rejected_preserves_victim_ticket() {
        let store = StateStore::open_in_memory();
        let owner = key(110);
        let attacker = key(111);
        fund(&store, &owner, 500);
        fund(&store, &attacker, 500);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let mut payment = DrcPaymentTx::unsigned_v4(
            attacker.address(),
            key(112).address(),
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
        sign_drc_payment_bound(&mut payment, &attacker, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &attacker);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        assert!(
            apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                .is_err()
        );
        assert_eq!(
            load_drc_account_tickets(&store, &owner.address()).unwrap(),
            vec![seq]
        );
    }

    #[test]
    fn duplicate_consumer_acceptance_borsh_stable() {
        let store = StateStore::open_in_memory();
        let owner = key(120);
        let peer = key(121);
        fund(&store, &owner, 1_000_000);
        let ctx = auth();
        let seq = mint_ticket(&store, &owner, &ctx, vec![Hash::ZERO]).0;
        let mut transfer = AccountTransfer {
            version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            from: owner.address(),
            to: peer.address(),
            amount: Amount::from_base_units(1),
            fee: Amount::ZERO,
            nonce: 0,
            account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut payment = DrcPaymentTx::unsigned_v4(
            owner.address(),
            peer.address(),
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
        sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
        let mut block = coinbase(vec![Hash::ZERO], &owner);
        block.account_transfers.push(transfer);
        block.drc_payments.push(payment);
        block.header.tx_root = block.compute_body_root();
        let bytes = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        let result =
            apply_block_batched_virtual_at_blue_score(&store, &decoded, 50, Some(&ctx), 50)
                .unwrap();
        let acceptance = result.acceptance.clone();
        let bytes2 = borsh::to_vec(&acceptance).unwrap();
        let acceptance2 = crate::acceptance::BlockAcceptanceRecord::from_bytes(&bytes2).unwrap();
        assert_eq!(acceptance, acceptance2);
        assert_eq!(
            acceptance.account_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert_eq!(
            acceptance.payment_statuses,
            vec![TransactionAcceptance::ConflictLost]
        );
    }

    #[test]
    fn invariant_sequence_create_spend_semantic_reject_rollback() {
        use agora_crypto::{
            sign_account_transfer_bound, sign_drc_account_policy_bound,
            sign_drc_deposit_preauth_bound, sign_drc_payment_bound, sign_drc_regular_key_bound,
            sign_drc_signer_list_bound, sign_stake_tx_bound,
        };

        let store = StateStore::open_in_memory();
        let owner = key(100);
        let peer = key(101);
        fund(&store, &owner, 10_000_000);
        let ctx = auth();
        let mut live: HashSet<u64> = HashSet::new();
        let mut parents = vec![Hash::ZERO];

        for round in 0u64..7 {
            let nonce = load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce;
            let create = signed_create(&owner, nonce, &ctx);
            let seq = nonce + 1;
            let mut block = coinbase(parents.clone(), &owner);
            block.drc_ticket_creates.push(create);
            block.header.tx_root = block.compute_body_root();
            store
                .write_batch(
                    apply_block_batched_with_auth_at_blue_score(&store, &block, 50, Some(&ctx), 50)
                        .unwrap()
                        .batch,
                )
                .unwrap();
            live.insert(seq);
            assert!(live.len() <= DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT);

            let before_nonce = load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .nonce;
            let tickets_before = load_drc_account_tickets(&store, &owner.address()).unwrap();
            let ticket_root = drc_ticket_root(&store).unwrap();
            let state_root = compose_trident_state_root(&store, &TIP).unwrap();
            let balance_before = load_account(&store, NativeAssetId::DRC, &owner.address())
                .unwrap()
                .balance;

            let mut reject_block = coinbase(vec![block.id()], &owner);
            match round {
                0 => {
                    let mut transfer = AccountTransfer {
                        version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
                        asset: NativeAssetId::DRC,
                        from: owner.address(),
                        to: peer.address(),
                        amount: Amount::from_base_units(10_000_000_000),
                        fee: Amount::ZERO,
                        nonce: 0,
                        account_sequence: Some(DrcAccountSequenceSelector::ticket(seq)),
                        public_key: Vec::new(),
                        signature: Vec::new(),
                        multisign: None,
                    };
                    sign_account_transfer_bound(&mut transfer, &owner, &ctx.chain_id, &ctx.genesis)
                        .unwrap();
                    reject_block.account_transfers.push(transfer);
                }
                1 => {
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
                    reject_block.stake_ops.push(stake);
                }
                2 => {
                    let mut rk = DrcRegularKeyTx::set(
                        owner.address(),
                        owner.address(),
                        owner.public_key_bytes().to_vec(),
                        Amount::ZERO,
                        0,
                    );
                    rk.version = DRC_REGULAR_KEY_TICKET_TX_VERSION;
                    rk.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
                    assert!(sign_drc_regular_key_bound(
                        &mut rk,
                        &owner,
                        &ctx.chain_id,
                        &ctx.genesis
                    )
                    .is_err());
                    reject_block.drc_regular_keys.push(rk);
                }
                3 => {
                    let mut sl = DrcSignerListTx::unsigned_set(
                        owner.address(),
                        agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
                            signer: peer.address(),
                            weight: 1,
                        }]),
                        1,
                        Amount::from_base_units(50_000_000),
                        0,
                    );
                    sl.version = DRC_SIGNER_LIST_TICKET_TX_VERSION;
                    sl.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
                    sign_drc_signer_list_bound(&mut sl, &owner, &ctx.chain_id, &ctx.genesis)
                        .unwrap();
                    reject_block.drc_signer_lists.push(sl);
                }
                4 => {
                    let mut policy = DrcAccountPolicyTx::set_master_key_disabled(
                        owner.address(),
                        Amount::ZERO,
                        0,
                    );
                    policy.version = DRC_ACCOUNT_POLICY_TICKET_TX_VERSION;
                    policy.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
                    sign_drc_account_policy_bound(&mut policy, &owner, &ctx.chain_id, &ctx.genesis)
                        .unwrap();
                    reject_block.drc_account_policies.push(policy);
                }
                5 => {
                    let mut revoke = DrcDepositPreauthTx::unauthorize(
                        owner.address(),
                        peer.address(),
                        Amount::ZERO,
                        0,
                    );
                    revoke.version = DRC_DEPOSIT_PREAUTH_TICKET_TX_VERSION;
                    revoke.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
                    sign_drc_deposit_preauth_bound(
                        &mut revoke,
                        &owner,
                        &ctx.chain_id,
                        &ctx.genesis,
                    )
                    .unwrap();
                    reject_block.drc_deposit_preauths.push(revoke);
                }
                _ => {
                    let mut payment = DrcPaymentTx::unsigned_v4(
                        owner.address(),
                        peer.address(),
                        Amount::from_base_units(50_000_000_000),
                        Amount::ZERO,
                        None,
                        None,
                        Hash::ZERO,
                        0,
                        None,
                    );
                    payment.version = DRC_PAYMENT_TICKET_VERSION;
                    payment.account_sequence = Some(DrcAccountSequenceSelector::ticket(seq));
                    sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis)
                        .unwrap();
                    reject_block.drc_payments.push(payment);
                }
            }
            reject_block.header.tx_root = reject_block.compute_body_root();
            assert!(apply_block_batched_with_auth_at_blue_score(
                &store,
                &reject_block,
                50,
                Some(&ctx),
                50
            )
            .is_err());
            assert_eq!(
                load_drc_account_tickets(&store, &owner.address()).unwrap(),
                tickets_before
            );
            assert_eq!(
                load_account(&store, NativeAssetId::DRC, &owner.address())
                    .unwrap()
                    .nonce,
                before_nonce
            );
            assert_eq!(drc_ticket_root(&store).unwrap(), ticket_root);
            assert_eq!(
                compose_trident_state_root(&store, &TIP).unwrap(),
                state_root
            );
            assert_eq!(
                load_account(&store, NativeAssetId::DRC, &owner.address())
                    .unwrap()
                    .balance,
                balance_before
            );

            let mut payment = DrcPaymentTx::unsigned_v4(
                owner.address(),
                key((102 + round as u8) % 200).address(),
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
            sign_drc_payment_bound(&mut payment, &owner, &ctx.chain_id, &ctx.genesis).unwrap();
            let mut spend_block = coinbase(vec![block.id()], &owner);
            spend_block.drc_payments.push(payment);
            spend_block.header.tx_root = spend_block.compute_body_root();
            let result = apply_block_batched_with_auth_at_blue_score(
                &store,
                &spend_block,
                50,
                Some(&ctx),
                50,
            )
            .unwrap();
            store.write_batch(result.batch).unwrap();
            assert!(live.remove(&seq));
            parents = vec![spend_block.id()];
        }
        assert!(live.is_empty());
    }
}
