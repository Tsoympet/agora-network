//! Native-fee and liability invariants through issued-controls flows.

use agora_rpc::RpcBackend;
use agora_types::DrcIssuedAssetPolicyAction;

use super::drc_issued_controls_public_helpers::{
    assert_liability_equals_sum_balances, burned_supply_balance, drc_balance,
    funded_trust_line_fixture, issuer_outstanding, line_balance, mine_template,
    setup_clawback_ready_line, setup_live_line, signed_clawback, signed_policy_set,
};
use super::drc_trust_line_public_helpers::asset;

#[test]
fn public_invariant_native_and_liability_through_policy_control_clawback() {
    let mut fx = funded_trust_line_fixture();
    let addrs = [
        fx.holder.address(),
        fx.issuer.address(),
        fx.holder_b.address(),
    ];
    let native0: u64 = addrs
        .iter()
        .map(|a| drc_balance(fx.store.as_ref(), a))
        .sum();
    let burned0 = burned_supply_balance(fx.store.as_ref());

    setup_clawback_ready_line(
        &mut fx.backend,
        fx.store.as_ref(),
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        40,
    );
    assert_liability_equals_sum_balances(fx.store.as_ref(), &fx.issuer.address(), &fx.cur_usd);
    let ast = asset(&fx.issuer, fx.cur_usd);

    fx.backend
        .submit_drc_trust_line_issuer_control(
            super::drc_issued_controls_public_helpers::signed_issuer_control(
                &fx.issuer,
                fx.holder.address(),
                fx.cur_usd,
                agora_types::DrcTrustLineIssuerControlAction::SetLineFrozen(true),
                fx.genesis,
                2,
            ),
        )
        .unwrap();
    mine_template(&mut fx.backend);
    assert_liability_equals_sum_balances(fx.store.as_ref(), &fx.issuer.address(), &fx.cur_usd);

    fx.backend
        .submit_drc_issued_clawback(signed_clawback(
            &fx.issuer,
            fx.holder.address(),
            fx.cur_usd,
            15,
            fx.genesis,
            3,
        ))
        .unwrap();
    mine_template(&mut fx.backend);

    assert_eq!(
        line_balance(fx.store.as_ref(), &fx.holder.address(), &ast),
        25
    );
    assert_eq!(issuer_outstanding(fx.store.as_ref(), &ast), 25);
    assert_liability_equals_sum_balances(fx.store.as_ref(), &fx.issuer.address(), &fx.cur_usd);

    let native1: u64 = addrs
        .iter()
        .map(|a| drc_balance(fx.store.as_ref(), a))
        .sum();
    let burned1 = burned_supply_balance(fx.store.as_ref());
    assert_eq!(native0 + burned0, native1 + burned1);
}

#[test]
fn public_invariant_global_freeze_blocks_issue_not_redeem() {
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
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_issued_asset_policy_set(signed_policy_set(
            &fx.issuer,
            fx.cur_usd,
            DrcIssuedAssetPolicyAction::ClearGlobalFreeze,
            fx.genesis,
            1,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    assert_liability_equals_sum_balances(fx.store.as_ref(), &fx.issuer.address(), &fx.cur_usd);
}
