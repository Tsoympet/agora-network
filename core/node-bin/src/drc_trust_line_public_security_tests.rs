//! NodeBackend public-admission security for trust lines and issued transfers.

use agora_crypto::{
    sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound, sign_drc_issued_transfer_bound,
    sign_drc_trust_line_set_bound, KeyPair,
};
use agora_rpc::RpcBackend;
use agora_types::{
    Amount, DrcAccountPolicyTx, DrcAccountSequenceSelector, DrcDepositPreauthAction,
    DrcDepositPreauthTx, Hash,
    DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION, DRC_TRUST_LINE_SET_TICKET_VERSION,
};

use super::drc_trust_line_public_helpers::{
    assert_template_has_no_invalid_trust_line_lanes, asset, funded_trust_line_fixture,
    issuer_liability_mutation_reserved, mempool_len, mine_template, setup_live_line,
    signed_issued_transfer, signed_trust_line_set, trust_line_meta_reserved,
    trust_line_mutation_reserved, CHAIN,
};

#[test]
fn public_admission_rejects_nonzero_invoice_and_clears_mempool() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        1_000,
        0,
    );
    let mut bad = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        10,
        fx.genesis,
        0,
    );
    bad.invoice_id = Hash([1u8; 32]);
    assert!(fx.backend.submit_drc_issued_transfer(bad).is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
    let template = fx.backend.get_block_template().unwrap();
    assert!(template.drc_issued_transfers.is_empty());
    assert!(assert_template_has_no_invalid_trust_line_lanes(&template));
}

#[test]
fn public_admission_rejects_future_version_trust_line_set_ticket_selector() {
    let mut fx = funded_trust_line_fixture();
    let mut tx = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        fx.genesis,
        0,
    );
    tx.version = DRC_TRUST_LINE_SET_TICKET_VERSION + 1;
    tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(2));
    assert!(fx.backend.submit_drc_trust_line_set(tx).is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
}

#[test]
fn public_admission_selector_tamper_after_sign_rejected() {
    let mut fx = funded_trust_line_fixture();
    let mut tx = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        500,
        fx.genesis,
        0,
    );
    tx.nonce = tx.nonce.wrapping_add(1);
    assert!(fx.backend.submit_drc_trust_line_set(tx).is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
}

#[test]
fn public_admission_require_dest_tag_including_zero_tag() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder_b,
        &fx.issuer,
        fx.cur_usd,
        1_000,
        0,
    );
    let mut policy = DrcAccountPolicyTx::set_require_destination_tag(
        fx.holder_b.address(),
        Amount::from_base_units(1),
        1,
    );
    sign_drc_account_policy_bound(&mut policy, &fx.holder_b, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_account_policy(policy).unwrap();
    mine_template(&mut fx.backend);

    let mut no_tag = signed_issued_transfer(
        &fx.issuer,
        fx.holder_b.address(),
        &fx.issuer,
        fx.cur_usd,
        5,
        fx.genesis,
        0,
    );
    assert!(fx.backend.submit_drc_issued_transfer(no_tag.clone()).is_err());
    no_tag.destination_tag = Some(0);
    no_tag.public_key.clear();
    no_tag.signature.clear();
    sign_drc_issued_transfer_bound(&mut no_tag, &fx.issuer, CHAIN, &fx.genesis).unwrap();
    assert!(fx.backend.submit_drc_issued_transfer(no_tag).is_ok());
}

#[test]
fn public_admission_deposit_auth_issue_holder_transfer_redeem() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        1_000,
        0,
    );
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            20,
            fx.genesis,
            0,
        ))
        .is_ok());
    mine_template(&mut fx.backend);

    let mut policy = DrcAccountPolicyTx::set_deposit_auth_required(
        fx.holder.address(),
        Amount::from_base_units(1),
        1,
    );
    sign_drc_account_policy_bound(&mut policy, &fx.holder, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_account_policy(policy).unwrap();
    mine_template(&mut fx.backend);

    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder_b,
        &fx.issuer,
        fx.cur_usd,
        1_000,
        0,
    );
    fx.backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder_b.address(),
            &fx.issuer,
            fx.cur_usd,
            50,
            fx.genesis,
            1,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.holder_b,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            5,
            fx.genesis,
            1,
        ))
        .is_err());

    let mut preauth = DrcDepositPreauthTx {
        version: 1,
        owner: fx.holder.address(),
        authorized_source: fx.holder_b.address(),
        action: DrcDepositPreauthAction::Authorize,
        fee: Amount::from_base_units(1),
        nonce: 2,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_deposit_preauth_bound(&mut preauth, &fx.holder, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_deposit_preauth(preauth).unwrap();
    mine_template(&mut fx.backend);
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.holder_b,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            5,
            fx.genesis,
            1,
        ))
        .is_ok());
    mine_template(&mut fx.backend);
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.holder,
            fx.issuer.address(),
            &fx.issuer,
            fx.cur_usd,
            5,
            fx.genesis,
            3,
        ))
        .is_ok());
}

#[test]
fn public_admission_pending_line_set_reserves_meta_and_blocks_transfer() {
    let mut fx = funded_trust_line_fixture();
    let set = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        fx.genesis,
        0,
    );
    fx.backend.submit_drc_trust_line_set(set).unwrap();
    let ast = asset(&fx.issuer, fx.cur_usd);
    assert!(trust_line_mutation_reserved(&fx.backend, &fx.holder.address(), &ast));
    assert!(trust_line_meta_reserved(&fx.backend, &fx.holder.address(), &ast));
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            1,
            fx.genesis,
            0,
        ))
        .is_err());
    mine_template(&mut fx.backend);
    assert!(!trust_line_mutation_reserved(&fx.backend, &fx.holder.address(), &ast));
}

#[test]
fn public_admission_liability_mutation_serializes_issue_and_redeem() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        1_000,
        0,
    );
    let ast = asset(&fx.issuer, fx.cur_usd);
    fx.backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            10,
            fx.genesis,
            0,
        ))
        .unwrap();
    assert!(issuer_liability_mutation_reserved(&fx.backend, &ast));
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.holder,
            fx.issuer.address(),
            &fx.issuer,
            fx.cur_usd,
            5,
            fx.genesis,
            1,
        ))
        .is_err());
}

#[test]
fn public_admission_rejects_future_issued_transfer_ticket_version() {
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
    let mut tx = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        1,
        fx.genesis,
        0,
    );
    tx.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION + 1;
    tx.account_sequence = Some(DrcAccountSequenceSelector::ticket(2));
    assert!(fx.backend.submit_drc_issued_transfer(tx).is_err());
}

#[test]
fn public_admission_disabled_master_rejected_regular_key_succeeds() {
    use agora_crypto::sign_drc_regular_key_bound;
    use agora_types::{Amount, DrcAccountPolicyTx, DrcRegularKeyTx};

    let mut fx = funded_trust_line_fixture();
    let regular = KeyPair::from_secret_bytes(&[0x61; 32]).unwrap();
    let mut reg = DrcRegularKeyTx::set(
        fx.holder.address(),
        regular.address(),
        regular.public_key_bytes().to_vec(),
        Amount::from_base_units(1),
        0,
    );
    sign_drc_regular_key_bound(&mut reg, &fx.holder, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_regular_key(reg).unwrap();
    mine_template(&mut fx.backend);

    let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
        fx.holder.address(),
        Amount::from_base_units(1),
        1,
    );
    sign_drc_account_policy_bound(&mut disable, &fx.holder, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_account_policy(disable).unwrap();
    mine_template(&mut fx.backend);

    let mut set = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        fx.genesis,
        2,
    );
    assert!(fx.backend.submit_drc_trust_line_set(set.clone()).is_err());
    sign_drc_trust_line_set_bound(&mut set, &regular, CHAIN, &fx.genesis).unwrap();
    assert!(fx.backend.submit_drc_trust_line_set(set).is_ok());
}

#[test]
fn public_admission_missing_line_and_limit_overflow_rejected() {
    let mut fx = funded_trust_line_fixture();
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            1,
            fx.genesis,
            0,
        ))
        .is_err());
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        5,
        0,
    );
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            6,
            fx.genesis,
            0,
        ))
        .is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
}

#[test]
fn public_admission_multisign_missing_attachment_never_admitted() {
    use agora_types::{DrcMultisignAuth, DrcMultisignEntry, DRC_MULTISIGN_AUTH_VERSION};

    let mut fx = funded_trust_line_fixture();
    let mut set = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        50,
        fx.genesis,
        0,
    );
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
}

#[test]
fn public_admission_ticket_cross_owner_rejected() {
    use agora_crypto::sign_drc_ticket_create_bound;
    use agora_types::{Amount, DrcAccountSequenceSelector, DrcTicketCreateTx};

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
    let mut ticket = DrcTicketCreateTx::unsigned(fx.holder.address(), Amount::from_base_units(1), 1);
    sign_drc_ticket_create_bound(&mut ticket, &fx.holder, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_ticket_create(ticket).unwrap();
    mine_template(&mut fx.backend);

    let mut issue = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        1,
        fx.genesis,
        0,
    );
    issue.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION;
    issue.account_sequence = Some(DrcAccountSequenceSelector::ticket(2));
    sign_drc_issued_transfer_bound(&mut issue, &fx.issuer, CHAIN, &fx.genesis).unwrap();
    assert!(fx.backend.submit_drc_issued_transfer(issue).is_err());
}

#[test]
fn public_admission_nonce_reservation_released_after_reject() {
    use super::drc_trust_line_public_helpers::account_reserved;

    let mut fx = funded_trust_line_fixture();
    let mut set = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        fx.genesis,
        0,
    );
    set.fee = Amount::from_base_units(10_000_000);
    assert!(fx.backend.submit_drc_trust_line_set(set).is_err());
    assert!(!account_reserved(&fx.backend, &fx.holder.address()));
}
