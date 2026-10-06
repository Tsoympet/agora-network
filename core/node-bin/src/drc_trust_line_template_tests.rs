//! Block template ordering and fail-closed lanes for trust lines.

use agora_rpc::RpcBackend;
use agora_types::{DrcMultisignAuth, DrcMultisignEntry, Hash, DRC_MULTISIGN_AUTH_VERSION};

use super::drc_trust_line_public_helpers::{
    assert_template_has_no_invalid_trust_line_lanes, funded_trust_line_fixture, mine_template,
    setup_live_line, signed_issued_transfer, signed_trust_line_set,
};

#[test]
fn template_selects_trust_line_set_before_issued_transfer() {
    let mut fx = funded_trust_line_fixture();
    let set = signed_trust_line_set(&fx.holder, &fx.issuer, fx.cur_usd, 100, fx.genesis, 0);
    fx.backend.submit_drc_trust_line_set(set).unwrap();
    fx.backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            1,
            fx.genesis,
            0,
        ))
        .unwrap_err();
    let template = fx.backend.get_block_template().unwrap();
    assert_eq!(template.drc_trust_line_sets.len(), 1);
    assert!(template.drc_issued_transfers.is_empty());
    assert!(assert_template_has_no_invalid_trust_line_lanes(&template));
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            1,
            fx.genesis,
            0,
        ))
        .unwrap();
    let template2 = fx.backend.get_block_template().unwrap();
    assert!(template2.drc_trust_line_sets.is_empty());
    assert_eq!(template2.drc_issued_transfers.len(), 1);
}

#[test]
fn template_never_includes_invalid_multisign_attachment_set() {
    let mut fx = funded_trust_line_fixture();
    let mut set = signed_trust_line_set(&fx.holder, &fx.issuer, fx.cur_usd, 50, fx.genesis, 0);
    set.multisign = Some(DrcMultisignAuth {
        version: DRC_MULTISIGN_AUTH_VERSION,
        signing_for: fx.holder.address(),
        signatures: vec![DrcMultisignEntry {
            signer: fx.holder.address(),
            public_key: fx.holder.public_key_bytes().to_vec(),
            signature: vec![0u8; 64],
        }],
    });
    assert!(fx.backend.submit_drc_trust_line_set(set).is_err());
    let template = fx.backend.get_block_template().unwrap();
    assert!(template.drc_trust_line_sets.is_empty());
    assert!(assert_template_has_no_invalid_trust_line_lanes(&template));
}

#[test]
fn template_after_live_line_only_valid_transfers() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        20,
        0,
    );
    fx.backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            5,
            fx.genesis,
            0,
        ))
        .unwrap();
    let template = fx.backend.get_block_template().unwrap();
    assert_eq!(template.drc_issued_transfers.len(), 1);
    assert_eq!(template.drc_issued_transfers[0].invoice_id, Hash::ZERO);
    assert!(assert_template_has_no_invalid_trust_line_lanes(&template));
}
