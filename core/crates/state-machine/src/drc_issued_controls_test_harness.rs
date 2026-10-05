//! Helpers for issued-controls consensus/security matrices.

#[cfg(test)]
pub mod support {
    use agora_crypto::{
        sign_drc_issued_asset_policy_set_bound, sign_drc_issued_clawback_bound,
        sign_drc_trust_line_issuer_control_bound, KeyPair,
    };
    use agora_types::{
        Amount, Block, DrcIssuedAssetPolicyAction, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx,
        DrcTrustLineIssuerControlAction, DrcTrustLineIssuerControlTx, Hash, IssuedAmount,
        IssuedAssetId, IssuedCurrencyCode, NativeAssetId, DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
        DRC_ISSUED_CLAWBACK_TX_VERSION, DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
    };

    use crate::accounts::{load_account, AccountJournal};
    use crate::apply::{apply_block_batched_with_auth_at_blue_score, TxAuthContext};
    use crate::drc_issued_controls::{drc_issued_controls_root, load_drc_issued_asset_policy};
    use crate::drc_trust_line::{
        drc_trust_line_root, load_drc_trust_line_live, sum_holder_balances_for_asset,
    };
    use crate::store::WriteBatch;
    use crate::{StateError, StateStore};

    pub use crate::drc_trust_line_test_harness::support::{
        apply_block, apply_block_journal, asset, auth, coinbase, fund, issuer_outstanding, key,
        line_balance, mint_ticket, revert_journal, setup_live_line, signed_issued_transfer,
        signed_trust_line_set, std_code,
    };

    pub fn signed_policy_set(
        issuer: &KeyPair,
        currency: IssuedCurrencyCode,
        action: DrcIssuedAssetPolicyAction,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcIssuedAssetPolicySetTx {
        let mut tx = DrcIssuedAssetPolicySetTx {
            version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
            issuer: issuer.address(),
            currency,
            action,
            fee: Amount::from_base_units(fee),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_asset_policy_set_bound(&mut tx, issuer, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        tx
    }

    pub fn signed_issuer_control(
        issuer: &KeyPair,
        holder: agora_types::Address,
        currency: IssuedCurrencyCode,
        action: DrcTrustLineIssuerControlAction,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcTrustLineIssuerControlTx {
        let mut tx = DrcTrustLineIssuerControlTx {
            version: DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
            issuer: issuer.address(),
            holder,
            currency,
            action,
            fee: Amount::from_base_units(fee),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_trust_line_issuer_control_bound(&mut tx, issuer, &ctx.chain_id, &ctx.genesis)
            .unwrap();
        tx
    }

    pub fn signed_clawback(
        issuer: &KeyPair,
        holder: agora_types::Address,
        currency: IssuedCurrencyCode,
        amount: u64,
        fee: u64,
        nonce: u64,
        ctx: &TxAuthContext,
    ) -> DrcIssuedClawbackTx {
        let mut tx = DrcIssuedClawbackTx {
            version: DRC_ISSUED_CLAWBACK_TX_VERSION,
            issuer: issuer.address(),
            holder,
            currency,
            amount: IssuedAmount::from_units(amount),
            fee: Amount::from_base_units(fee),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_clawback_bound(&mut tx, issuer, &ctx.chain_id, &ctx.genesis).unwrap();
        tx
    }

    pub fn apply_policy_direct(
        store: &StateStore,
        tx: &DrcIssuedAssetPolicySetTx,
        ctx: &TxAuthContext,
        blue: u64,
    ) -> Result<(), StateError> {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_issued_asset_policy_set(store, tx, ctx, &mut batch, &mut journal, blue)?;
        store.write_batch(batch)
    }

    pub fn apply_issuer_control_direct(
        store: &StateStore,
        tx: &DrcTrustLineIssuerControlTx,
        ctx: &TxAuthContext,
        blue: u64,
    ) -> Result<(), StateError> {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_trust_line_issuer_control(store, tx, ctx, &mut batch, &mut journal, blue)?;
        store.write_batch(batch)
    }

    pub fn apply_clawback_direct(
        store: &StateStore,
        tx: &DrcIssuedClawbackTx,
        ctx: &TxAuthContext,
        blue: u64,
    ) -> Result<(), StateError> {
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_issued_clawback(store, tx, ctx, &mut batch, &mut journal, blue)?;
        store.write_batch(batch)
    }

    pub fn issuer_drc_nonce(store: &StateStore, issuer: &KeyPair) -> u64 {
        load_account(store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce
    }

    pub fn assert_liability_matches_holders(store: &StateStore, ast: &IssuedAssetId) {
        assert_eq!(
            sum_holder_balances_for_asset(store, ast)
                .unwrap()
                .as_units(),
            issuer_outstanding(store, ast)
        );
    }

    pub fn controls_roots(store: &StateStore) -> (Hash, Hash) {
        (
            drc_issued_controls_root(store).unwrap(),
            drc_trust_line_root(store).unwrap(),
        )
    }

    pub fn reject_apply_preserving(
        store: &StateStore,
        block: &Block,
        ctx: &TxAuthContext,
        roots_before: (Hash, Hash),
        issuer: &KeyPair,
        ast: &IssuedAssetId,
    ) {
        let nonce_before = issuer_drc_nonce(store, issuer);
        if agora_types::validate_drc_multisign_attachment_lane(block, &ctx.chain_id, &ctx.genesis)
            .is_err()
        {
            assert_eq!(controls_roots(store), roots_before);
            assert_eq!(issuer_drc_nonce(store, issuer), nonce_before);
            return;
        }
        assert!(
            apply_block_batched_with_auth_at_blue_score(store, block, 50, Some(ctx), 50,).is_err()
        );
        assert_eq!(controls_roots(store), roots_before);
        assert_eq!(issuer_drc_nonce(store, issuer), nonce_before);
        assert_eq!(
            load_drc_issued_asset_policy(store, ast).unwrap(),
            load_drc_issued_asset_policy(store, ast).unwrap()
        );
        assert_liability_matches_holders(store, ast);
    }

    pub fn line_on_disk_version(
        store: &StateStore,
        holder: agora_types::Address,
        ast: &IssuedAssetId,
    ) -> u32 {
        load_drc_trust_line_live(store, &holder, ast)
            .unwrap()
            .unwrap()
            .version
    }

    pub fn line_on_disk_authorized(
        store: &StateStore,
        holder: agora_types::Address,
        ast: &IssuedAssetId,
    ) -> bool {
        load_drc_trust_line_live(store, &holder, ast)
            .unwrap()
            .unwrap()
            .authorized
    }

    /// Rewrites an existing live line as legacy on-disk v1 bytes (no v2 flag fields) for migration tests.
    pub fn persist_legacy_v1_trust_line_on_disk(
        store: &StateStore,
        holder: agora_types::Address,
        ast: &IssuedAssetId,
    ) {
        use crate::columns::ColumnFamily;
        use crate::drc_trust_line::trust_line_key;
        use borsh::BorshSerialize;

        let line = load_drc_trust_line_live(store, &holder, ast)
            .unwrap()
            .expect("line must exist");
        let mut bytes = Vec::new();
        BorshSerialize::serialize(&agora_types::DRC_TRUST_LINE_LIVE_STATE_VERSION, &mut bytes)
            .unwrap();
        BorshSerialize::serialize(&line.holder, &mut bytes).unwrap();
        BorshSerialize::serialize(&line.asset, &mut bytes).unwrap();
        BorshSerialize::serialize(&line.limit, &mut bytes).unwrap();
        BorshSerialize::serialize(&line.balance, &mut bytes).unwrap();
        let mut batch = WriteBatch::new();
        batch.put_cf(ColumnFamily::Meta, &trust_line_key(&holder, ast), &bytes);
        store.write_batch(batch).unwrap();
        assert_eq!(
            line_on_disk_version(store, holder, ast),
            agora_types::DRC_TRUST_LINE_LIVE_STATE_VERSION
        );
    }
}

#[cfg(test)]
pub mod multisign {
    use agora_crypto::{
        sign_drc_issued_asset_policy_set_bound, sign_drc_issued_clawback_bound,
        sign_drc_trust_line_issuer_control_bound, KeyPair,
    };
    use agora_types::{
        materialize_drc_multisign_attachments, Amount, Block, DrcIssuedAssetPolicyAction,
        DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx, DrcTrustLineIssuerControlAction,
        DrcTrustLineIssuerControlTx, Hash, IssuedAmount, IssuedCurrencyCode,
        DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION, DRC_ISSUED_CLAWBACK_TX_VERSION,
        DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
    };

    use crate::accounts::load_account;
    use crate::apply::TxAuthContext;
    use crate::drc_trust_line_test_harness::multisign::{
        install_signer_list, install_signer_list_at_nonce, multisign_bundle,
    };
    use crate::drc_trust_line_test_harness::support::{coinbase, fund, std_code};
    use crate::StateStore;
    use agora_types::NativeAssetId;

    pub fn base_multisign_policy_set_block(
        store: &StateStore,
        issuer: &KeyPair,
        currency: IssuedCurrencyCode,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, issuer, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = DrcIssuedAssetPolicySetTx {
            version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
            issuer: issuer.address(),
            currency,
            action: DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            issuer.address(),
            &signing_bytes,
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], issuer);
        block.drc_issued_asset_policy_sets.push(tx);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        block
    }

    pub fn base_multisign_issuer_control_block(
        store: &StateStore,
        issuer: &KeyPair,
        holder: agora_types::Address,
        currency: IssuedCurrencyCode,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, issuer, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = DrcTrustLineIssuerControlTx {
            version: DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
            issuer: issuer.address(),
            holder,
            currency,
            action: DrcTrustLineIssuerControlAction::SetLineFrozen(true),
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            issuer.address(),
            &signing_bytes,
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], issuer);
        block.drc_trust_line_issuer_controls.push(tx);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        block
    }

    pub fn base_multisign_clawback_block(
        store: &StateStore,
        issuer: &KeyPair,
        holder: agora_types::Address,
        currency: IssuedCurrencyCode,
        signers: &[(&KeyPair, u16)],
        ctx: &TxAuthContext,
    ) -> Block {
        install_signer_list(store, issuer, signers, ctx);
        let nonce = load_account(store, NativeAssetId::DRC, &issuer.address())
            .unwrap()
            .nonce;
        let mut tx = DrcIssuedClawbackTx {
            version: DRC_ISSUED_CLAWBACK_TX_VERSION,
            issuer: issuer.address(),
            holder,
            currency,
            amount: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        let signing_bytes = tx.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
        tx.multisign = Some(multisign_bundle(
            issuer.address(),
            &signing_bytes,
            signers,
            ctx,
        ));
        let mut block = coinbase(vec![Hash::ZERO], issuer);
        block.drc_issued_clawbacks.push(tx);
        materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
        block.header.tx_root = block.compute_body_root();
        block
    }

    pub fn resign_policy_master(block: &mut Block, issuer: &KeyPair, ctx: &TxAuthContext) {
        sign_drc_issued_asset_policy_set_bound(
            &mut block.drc_issued_asset_policy_sets[0],
            issuer,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
    }

    pub fn resign_issuer_control_master(block: &mut Block, issuer: &KeyPair, ctx: &TxAuthContext) {
        sign_drc_trust_line_issuer_control_bound(
            &mut block.drc_trust_line_issuer_controls[0],
            issuer,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
    }

    pub fn resign_clawback_master(block: &mut Block, issuer: &KeyPair, ctx: &TxAuthContext) {
        sign_drc_issued_clawback_bound(
            &mut block.drc_issued_clawbacks[0],
            issuer,
            &ctx.chain_id,
            &ctx.genesis,
        )
        .unwrap();
    }
}
