//! Block template ordering for issued-controls lanes.

use agora_rpc::RpcBackend;
use agora_types::{DrcIssuedAssetPolicyAction, DrcTrustLineIssuerControlAction};

use super::drc_issued_controls_public_helpers::{
    funded_trust_line_fixture, mine_template, setup_live_line, signed_issued_transfer,
    signed_issuer_control, signed_policy_set,
};

#[test]
fn template_selects_policy_before_issuer_control_before_transfer() {
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
            DrcIssuedAssetPolicyAction::EnableRequireAuth,
            fx.genesis,
            0,
        ))
        .unwrap();
    fx.backend
        .submit_drc_trust_line_issuer_control(signed_issuer_control(
            &fx.issuer,
            fx.holder.address(),
            fx.cur_usd,
            DrcTrustLineIssuerControlAction::AuthorizeHolder,
            fx.genesis,
            1,
        ))
        .unwrap_err();
    let template = fx.backend.get_block_template().unwrap();
    assert_eq!(template.drc_issued_asset_policy_sets.len(), 1);
    assert!(template.drc_trust_line_issuer_controls.is_empty());
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_trust_line_issuer_control(signed_issuer_control(
            &fx.issuer,
            fx.holder.address(),
            fx.cur_usd,
            DrcTrustLineIssuerControlAction::AuthorizeHolder,
            fx.genesis,
            1,
        ))
        .unwrap();
    let template2 = fx.backend.get_block_template().unwrap();
    assert!(template2.drc_issued_asset_policy_sets.is_empty());
    assert_eq!(template2.drc_trust_line_issuer_controls.len(), 1);
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            1,
            fx.genesis,
            2,
        ))
        .unwrap();
    let template3 = fx.backend.get_block_template().unwrap();
    assert_eq!(template3.drc_issued_transfers.len(), 1);
}
