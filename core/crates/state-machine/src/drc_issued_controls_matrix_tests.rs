//! Core matrices for issued-asset policy, freeze, auth, and clawback.

#[cfg(test)]
mod tests {
    use crate::{
        apply_drc_issued_asset_policy_set, apply_drc_issued_clawback, apply_drc_issued_transfer,
        apply_drc_trust_line_issuer_control, apply_drc_trust_line_set, credit_account_into,
        load_drc_issued_asset_policy, load_drc_issuer_liability, load_drc_trust_line_live,
        GenesisBuilder, StateStore, TxAuthContext, WriteBatch,
    };
    use agora_crypto::{
        sign_drc_issued_asset_policy_set_bound, sign_drc_issued_clawback_bound,
        sign_drc_issued_transfer_bound, sign_drc_trust_line_issuer_control_bound,
        sign_drc_trust_line_set_bound, KeyPair,
    };
    use agora_types::{
        Amount, DrcIssuedAssetPolicyAction, DrcTrustLineIssuerControlAction, Hash, IssuedAmount,
        IssuedCurrencyCode, NativeAssetId, DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
        DRC_ISSUED_CLAWBACK_TX_VERSION, DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
        DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
    };

    const CHAIN: &str = "agora-dev";

    fn ctx(genesis: Hash) -> TxAuthContext {
        TxAuthContext {
            chain_id: CHAIN.into(),
            genesis,
            data_availability_network_fingerprint: None,
        }
    }

    fn std_usd() -> IssuedCurrencyCode {
        IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0")
    }

    #[test]
    fn require_auth_blocks_issue_until_authorize() {
        let store = StateStore::open_in_memory();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let issuer = KeyPair::from_secret_bytes(&[0x21; 32]).unwrap();
        let holder = KeyPair::from_secret_bytes(&[0x22; 32]).unwrap();
        let mut batch = WriteBatch::new();
        for kp in [&issuer, &holder] {
            credit_account_into(
                &mut batch,
                &store,
                NativeAssetId::DRC,
                &kp.address(),
                Amount::from_base_units(100_000),
            )
            .unwrap();
        }
        store.write_batch(batch).unwrap();
        let auth = ctx(genesis);

        let mut policy = agora_types::DrcIssuedAssetPolicySetTx {
            version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
            issuer: issuer.address(),
            currency: std_usd(),
            action: DrcIssuedAssetPolicyAction::EnableRequireAuth,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_asset_policy_set_bound(&mut policy, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = crate::accounts::AccountJournal::default();
        apply_drc_issued_asset_policy_set(&store, &policy, &auth, &mut batch, &mut journal, 1)
            .unwrap();
        store.write_batch(batch).unwrap();
        assert!(
            load_drc_issued_asset_policy(&store, &policy.asset_id())
                .unwrap()
                .require_auth
        );

        let mut set = agora_types::DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency: std_usd(),
            limit: IssuedAmount::from_units(1_000),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_trust_line_set_bound(&mut set, &holder, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_trust_line_set(&store, &set, &auth, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let line = load_drc_trust_line_live(&store, &holder.address(), &set.asset_id())
            .unwrap()
            .unwrap();
        assert!(!line.authorized);

        let mut issue = agora_types::DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: issuer.address(),
            recipient: holder.address(),
            issuer: issuer.address(),
            currency: std_usd(),
            amount: IssuedAmount::from_units(10),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_transfer_bound(&mut issue, &issuer, CHAIN, &genesis).unwrap();
        issue.nonce = 1;
        sign_drc_issued_transfer_bound(&mut issue, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        assert!(
            apply_drc_issued_transfer(&store, &issue, &auth, 2, &mut batch, &mut journal).is_err()
        );

        let mut authz = agora_types::DrcTrustLineIssuerControlTx {
            version: DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
            issuer: issuer.address(),
            holder: holder.address(),
            currency: std_usd(),
            action: DrcTrustLineIssuerControlAction::AuthorizeHolder,
            fee: Amount::from_base_units(1),
            nonce: 1,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_trust_line_issuer_control_bound(&mut authz, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_trust_line_issuer_control(&store, &authz, &auth, &mut batch, &mut journal, 3)
            .unwrap();
        store.write_batch(batch).unwrap();
        issue.nonce = 2;
        sign_drc_issued_transfer_bound(&mut issue, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_issued_transfer(&store, &issue, &auth, 4, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
    }

    #[test]
    fn clawback_reduces_balance_and_liability_exactly() {
        let store = StateStore::open_in_memory();
        let genesis = GenesisBuilder::default().ignite(&store).unwrap();
        let issuer = KeyPair::from_secret_bytes(&[0x31; 32]).unwrap();
        let holder = KeyPair::from_secret_bytes(&[0x32; 32]).unwrap();
        let mut batch = WriteBatch::new();
        for kp in [&issuer, &holder] {
            credit_account_into(
                &mut batch,
                &store,
                NativeAssetId::DRC,
                &kp.address(),
                Amount::from_base_units(100_000),
            )
            .unwrap();
        }
        store.write_batch(batch).unwrap();
        let auth = ctx(genesis);
        let mut journal = crate::accounts::AccountJournal::default();

        let mut claw_pol = agora_types::DrcIssuedAssetPolicySetTx {
            version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
            issuer: issuer.address(),
            currency: std_usd(),
            action: DrcIssuedAssetPolicyAction::EnableClawback,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_asset_policy_set_bound(&mut claw_pol, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_issued_asset_policy_set(&store, &claw_pol, &auth, &mut batch, &mut journal, 1)
            .unwrap();
        store.write_batch(batch).unwrap();

        let mut set = agora_types::DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: holder.address(),
            issuer: issuer.address(),
            currency: std_usd(),
            limit: IssuedAmount::from_units(500),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_trust_line_set_bound(&mut set, &holder, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_trust_line_set(&store, &set, &auth, 1, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut issue = agora_types::DrcIssuedTransferTx {
            version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: issuer.address(),
            recipient: holder.address(),
            issuer: issuer.address(),
            currency: std_usd(),
            amount: IssuedAmount::from_units(100),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 0,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_transfer_bound(&mut issue, &issuer, CHAIN, &genesis).unwrap();
        issue.nonce = 1;
        sign_drc_issued_transfer_bound(&mut issue, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_issued_transfer(&store, &issue, &auth, 2, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut claw = agora_types::DrcIssuedClawbackTx {
            version: DRC_ISSUED_CLAWBACK_TX_VERSION,
            issuer: issuer.address(),
            holder: holder.address(),
            currency: std_usd(),
            amount: IssuedAmount::from_units(40),
            fee: Amount::from_base_units(1),
            nonce: 2,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        };
        sign_drc_issued_clawback_bound(&mut claw, &issuer, CHAIN, &genesis).unwrap();
        let mut batch = WriteBatch::new();
        apply_drc_issued_clawback(&store, &claw, &auth, &mut batch, &mut journal, 3).unwrap();
        store.write_batch(batch).unwrap();

        let line = load_drc_trust_line_live(&store, &holder.address(), &set.asset_id())
            .unwrap()
            .unwrap();
        assert_eq!(line.balance.as_units(), 60);
        assert_eq!(
            load_drc_issuer_liability(&store, &set.asset_id())
                .unwrap()
                .as_units(),
            60
        );
    }
}
