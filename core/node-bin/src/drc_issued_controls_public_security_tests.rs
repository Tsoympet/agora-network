//! NodeBackend public-admission security for issued-controls lanes.

use agora_crypto::{
    sign_drc_account_policy_bound, sign_drc_issued_asset_policy_set_bound,
    sign_drc_regular_key_bound, KeyPair,
};
use agora_rpc::RpcBackend;
use agora_types::{
    Amount, DrcAccountPolicyTx, DrcIssuedAssetPolicyAction, DrcRegularKeyTx,
    DrcTrustLineIssuerControlAction,
};

use super::drc_issued_controls_public_helpers::{
    account_reserved, assert_mining_template_pow_intact, asset, asset_policy_mutation_reserved,
    funded_trust_line_fixture, issuer_control_mutation_reserved, mempool_len, mine_template,
    setup_live_line, signed_issuer_control, signed_policy_set, CHAIN,
};

#[test]
fn public_admission_policy_tamper_after_sign_rejected() {
    let mut fx = funded_trust_line_fixture();
    let mut tx = signed_policy_set(
        &fx.issuer,
        fx.cur_usd,
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fx.genesis,
        0,
    );
    tx.nonce = tx.nonce.wrapping_add(1);
    assert!(fx.backend.submit_drc_issued_asset_policy_set(tx).is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
}

#[test]
fn public_admission_duplicate_policy_mutation_rejected() {
    let mut fx = funded_trust_line_fixture();
    let tx = signed_policy_set(
        &fx.issuer,
        fx.cur_usd,
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fx.genesis,
        0,
    );
    fx.backend
        .submit_drc_issued_asset_policy_set(tx.clone())
        .unwrap();
    assert!(asset_policy_mutation_reserved(
        &fx.backend,
        &asset(&fx.issuer, fx.cur_usd)
    ));
    assert!(fx.backend.submit_drc_issued_asset_policy_set(tx).is_err());
    assert_eq!(mempool_len(&fx.backend), 1);
}

#[test]
fn public_admission_pending_policy_blocks_issuer_control() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        0,
    );
    fx.backend
        .submit_drc_issued_asset_policy_set(signed_policy_set(
            &fx.issuer,
            fx.cur_usd,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            fx.genesis,
            0,
        ))
        .unwrap();
    let ctrl = signed_issuer_control(
        &fx.issuer,
        fx.holder.address(),
        fx.cur_usd,
        DrcTrustLineIssuerControlAction::SetLineFrozen(true),
        fx.genesis,
        0,
    );
    assert!(fx
        .backend
        .submit_drc_trust_line_issuer_control(ctrl)
        .is_err());
}

#[test]
fn public_admission_disabled_master_policy_rejected_regular_recovers() {
    let mut fx = funded_trust_line_fixture();
    let regular = KeyPair::from_secret_bytes(&[0x71; 32]).unwrap();
    let mut reg = DrcRegularKeyTx::set(
        fx.issuer.address(),
        regular.address(),
        regular.public_key_bytes().to_vec(),
        Amount::from_base_units(1),
        0,
    );
    sign_drc_regular_key_bound(&mut reg, &fx.issuer, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_regular_key(reg).unwrap();
    mine_template(&mut fx.backend);

    let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
        fx.issuer.address(),
        Amount::from_base_units(1),
        1,
    );
    sign_drc_account_policy_bound(&mut disable, &fx.issuer, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_account_policy(disable).unwrap();
    mine_template(&mut fx.backend);

    let mut policy = signed_policy_set(
        &fx.issuer,
        fx.cur_usd,
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fx.genesis,
        2,
    );
    assert!(fx
        .backend
        .submit_drc_issued_asset_policy_set(policy.clone())
        .is_err());
    sign_drc_issued_asset_policy_set_bound(&mut policy, &regular, CHAIN, &fx.genesis).unwrap();
    assert!(fx
        .backend
        .submit_drc_issued_asset_policy_set(policy)
        .is_ok());
}

#[test]
fn public_admission_issuer_control_reservations_cleared_on_mine() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        50,
        0,
    );
    let ast = asset(&fx.issuer, fx.cur_usd);
    let ctrl = signed_issuer_control(
        &fx.issuer,
        fx.holder.address(),
        fx.cur_usd,
        DrcTrustLineIssuerControlAction::SetLineFrozen(true),
        fx.genesis,
        0,
    );
    fx.backend
        .submit_drc_trust_line_issuer_control(ctrl)
        .unwrap();
    assert!(issuer_control_mutation_reserved(
        &fx.backend,
        &fx.holder.address(),
        &ast
    ));
    assert!(account_reserved(&fx.backend, &fx.issuer.address()));
    mine_template(&mut fx.backend);
    assert!(!issuer_control_mutation_reserved(
        &fx.backend,
        &fx.holder.address(),
        &ast
    ));
    assert!(!account_reserved(&fx.backend, &fx.issuer.address()));
}

#[test]
fn public_admission_mining_template_pow_intact_with_pending_policy() {
    let mut fx = funded_trust_line_fixture();
    fx.backend
        .submit_drc_issued_asset_policy_set(signed_policy_set(
            &fx.issuer,
            fx.cur_usd,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            fx.genesis,
            0,
        ))
        .unwrap();
    assert_mining_template_pow_intact(&fx.backend);
}
