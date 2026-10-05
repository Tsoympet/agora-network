//! NodeBackend reorg, reservation eviction, and resubmit for issued controls.

use agora_rpc::RpcBackend;
use agora_state_machine::load_drc_issued_asset_policy_receipt;
use agora_types::DrcIssuedAssetPolicyAction;

use super::drc_issued_controls_public_helpers::{
    assert_liability_equals_sum_balances, asset, burned_supply_balance, funded_trust_line_fixture,
    issuer_nonce, mempool_len, mine_template, setup_live_line, signed_policy_set,
    submit_lanes_at_parents, virtual_tip, CHAIN,
};
use crate::admit::BlockTemplateLanes;

#[test]
fn nodebackend_policy_inclusion_clears_asset_policy_reservation() {
    let mut fx = funded_trust_line_fixture();
    let policy = signed_policy_set(
        &fx.issuer,
        fx.cur_usd,
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fx.genesis,
        0,
    );
    let id = policy.policy_set_tx_id();
    fx.backend
        .submit_drc_issued_asset_policy_set(policy)
        .unwrap();
    assert_eq!(mempool_len(&fx.backend), 1);
    mine_template(&mut fx.backend);
    assert_eq!(mempool_len(&fx.backend), 0);
    assert!(load_drc_issued_asset_policy_receipt(fx.store.as_ref(), &id)
        .unwrap()
        .is_some());
}

#[test]
fn nodebackend_reorg_reverts_policy_resubmit_once() {
    let mut fx = funded_trust_line_fixture();
    let policy = signed_policy_set(
        &fx.issuer,
        fx.cur_usd,
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fx.genesis,
        0,
    );
    let policy_id = policy.policy_set_tx_id();
    let ast = asset(&fx.issuer, fx.cur_usd);
    let burned0 = burned_supply_balance(fx.store.as_ref());
    let fund_tip = virtual_tip(&fx.backend);
    let policy_tip = submit_lanes_at_parents(
        &mut fx.backend,
        &[fund_tip],
        1_000,
        BlockTemplateLanes {
            drc_issued_asset_policy_sets: std::slice::from_ref(&policy),
            ..BlockTemplateLanes::default()
        },
        70,
    );
    assert!(
        agora_state_machine::load_drc_issued_asset_policy(fx.store.as_ref(), &ast)
            .unwrap()
            .global_freeze
    );
    assert!(
        load_drc_issued_asset_policy_receipt(fx.store.as_ref(), &policy_id)
            .unwrap()
            .is_some()
    );
    let mut revert_tip = submit_lanes_at_parents(
        &mut fx.backend,
        &[fund_tip],
        2_000,
        BlockTemplateLanes::default(),
        71,
    );
    for extranonce in 72..100 {
        revert_tip = submit_lanes_at_parents(
            &mut fx.backend,
            &[revert_tip],
            1_000,
            BlockTemplateLanes::default(),
            extranonce,
        );
        if !agora_state_machine::load_drc_issued_asset_policy(fx.store.as_ref(), &ast)
            .unwrap()
            .global_freeze
        {
            break;
        }
    }
    assert!(
        !agora_state_machine::load_drc_issued_asset_policy(fx.store.as_ref(), &ast)
            .unwrap()
            .global_freeze
    );
    assert!(
        load_drc_issued_asset_policy_receipt(fx.store.as_ref(), &policy_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(burned_supply_balance(fx.store.as_ref()), burned0);
    assert_liability_equals_sum_balances(fx.store.as_ref(), &fx.issuer.address(), &fx.cur_usd);
    assert!(fx
        .backend
        .submit_drc_issued_asset_policy_set(policy.clone())
        .is_ok());
    assert!(fx
        .backend
        .submit_drc_issued_asset_policy_set(policy)
        .is_err());
    let _ = (policy_tip, revert_tip);
}

#[test]
fn nodebackend_pending_policy_blocks_transfer_admission() {
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
    fx.backend
        .submit_drc_issued_asset_policy_set(signed_policy_set(
            &fx.issuer,
            fx.cur_usd,
            DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
            fx.genesis,
            issuer_nonce(fx.store.as_ref(), &fx.issuer.address()),
        ))
        .unwrap();
    assert!(fx
        .backend
        .submit_drc_issued_transfer(
            super::drc_issued_controls_public_helpers::signed_issued_transfer(
                &fx.issuer,
                fx.holder.address(),
                &fx.issuer,
                fx.cur_usd,
                1,
                fx.genesis,
                issuer_nonce(fx.store.as_ref(), &fx.issuer.address()),
            )
        )
        .is_err());
    let _ = CHAIN;
}
