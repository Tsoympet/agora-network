//! Shared helpers for NodeBackend issued-controls public integration tests.

pub use super::drc_trust_line_public_helpers::{
    account_reserved, assert_liability_equals_sum_balances, asset, backend_config, boot_chain,
    burned_supply_balance, drc_balance, funded_trust_line_fixture, issuer_outstanding,
    line_balance, mempool_len, mine_template, setup_live_line, signed_issued_transfer,
    submit_lanes_at_parents, virtual_tip, CHAIN,
};

use agora_consensus::{LeadingZeroPow, PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
use agora_crypto::{
    sign_drc_issued_asset_policy_set_bound, sign_drc_issued_clawback_bound,
    sign_drc_trust_line_issuer_control_bound, KeyPair,
};
use agora_rpc::RpcBackend;
use agora_state_machine::load_drc_issued_asset_policy;
use agora_types::{
    Address, Amount, DrcIssuedAssetPolicyAction, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx,
    DrcTrustLineIssuerControlAction, DrcTrustLineIssuerControlTx, Hash, IssuedAssetId,
    IssuedCurrencyCode, DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION, DRC_ISSUED_CLAWBACK_TX_VERSION,
    DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
};

use super::NodeBackend;

pub fn signed_policy_set(
    issuer: &KeyPair,
    currency: IssuedCurrencyCode,
    action: DrcIssuedAssetPolicyAction,
    genesis: Hash,
    nonce: u64,
) -> DrcIssuedAssetPolicySetTx {
    let mut tx = DrcIssuedAssetPolicySetTx {
        version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
        issuer: issuer.address(),
        currency,
        action,
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_issued_asset_policy_set_bound(&mut tx, issuer, CHAIN, &genesis).unwrap();
    tx
}

pub fn signed_issuer_control(
    issuer: &KeyPair,
    holder: Address,
    currency: IssuedCurrencyCode,
    action: DrcTrustLineIssuerControlAction,
    genesis: Hash,
    nonce: u64,
) -> DrcTrustLineIssuerControlTx {
    let mut tx = DrcTrustLineIssuerControlTx {
        version: DRC_TRUST_LINE_ISSUER_CONTROL_TX_VERSION,
        issuer: issuer.address(),
        holder,
        currency,
        action,
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_trust_line_issuer_control_bound(&mut tx, issuer, CHAIN, &genesis).unwrap();
    tx
}

pub fn signed_clawback(
    issuer: &KeyPair,
    holder: Address,
    currency: IssuedCurrencyCode,
    amount: u64,
    genesis: Hash,
    nonce: u64,
) -> DrcIssuedClawbackTx {
    let mut tx = DrcIssuedClawbackTx {
        version: DRC_ISSUED_CLAWBACK_TX_VERSION,
        issuer: issuer.address(),
        holder,
        currency,
        amount: agora_types::IssuedAmount::from_units(amount),
        fee: Amount::from_base_units(1),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_issued_clawback_bound(&mut tx, issuer, CHAIN, &genesis).unwrap();
    tx
}

pub fn asset_policy_mutation_reserved(backend: &NodeBackend, ast: &IssuedAssetId) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .asset_policy_mutation_reserved(&ast.asset_key())
}

pub fn issuer_control_mutation_reserved(
    backend: &NodeBackend,
    holder: &Address,
    ast: &IssuedAssetId,
) -> bool {
    backend
        .test_mempool()
        .lock()
        .unwrap()
        .issuer_control_mutation_reserved(holder, &ast.asset_key())
}

pub fn issuer_nonce(store: &agora_state_machine::StateStore, issuer: &Address) -> u64 {
    agora_state_machine::load_account(store, agora_types::NativeAssetId::DRC, issuer)
        .unwrap()
        .nonce
}

pub fn setup_clawback_ready_line(
    backend: &mut NodeBackend,
    store: &agora_state_machine::StateStore,
    genesis: Hash,
    holder: &KeyPair,
    issuer: &KeyPair,
    currency: IssuedCurrencyCode,
    limit: u64,
    issue_amount: u64,
) {
    setup_live_line(backend, genesis, holder, issuer, currency, limit, 0);
    let policy = signed_policy_set(
        issuer,
        currency,
        DrcIssuedAssetPolicyAction::EnableClawback,
        genesis,
        0,
    );
    backend.submit_drc_issued_asset_policy_set(policy).unwrap();
    mine_template(backend);
    let issue = signed_issued_transfer(
        issuer,
        holder.address(),
        issuer,
        currency,
        issue_amount,
        genesis,
        1,
    );
    backend.submit_drc_issued_transfer(issue).unwrap();
    mine_template(backend);
    let ast = asset(issuer, currency);
    assert!(
        load_drc_issued_asset_policy(store, &ast)
            .unwrap()
            .clawback_enabled
    );
}

/// Mining template must still expose PoW-bound coinbase when issued-control lanes are pending.
pub fn assert_mining_template_pow_intact(backend: &NodeBackend) {
    let mut block = backend.get_block_template().unwrap();
    assert!(!block.transactions.is_empty());
    assert_eq!(block.header.tx_root, block.compute_body_root());
    block.header.nonce = 1;
    let pow = RandomXPowHasher.pow_hash(&block.header);
    LeadingZeroPow::new(PowAlgorithm::RandomX)
        .verify(&block.header, &pow)
        .unwrap();
}
