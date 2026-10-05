//! NodeBackend trust-line reorg, reservation eviction, and resubmit semantics.

use agora_crypto::sign_drc_ticket_create_bound;
use agora_rpc::RpcBackend;
use agora_state_machine::load_drc_issued_transfer_receipt;
use agora_types::{
    Amount, DrcAccountSequenceSelector, DrcTicketCreateTx, Hash,
    DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION,
};

use super::drc_trust_line_public_helpers::{
    account_reserved, assert_liability_equals_sum_balances,
    assert_template_has_no_invalid_trust_line_lanes, asset, drc_balance, issuer_outstanding,
    line_balance, mempool_len, mine_template, reorg_away_transfer_on_tip, reward_pool_balance,
    setup_live_line, signed_issued_transfer, signed_trust_line_set, submit_lanes_at_parents,
    ticket_consumer_reserved, trust_line_meta_reserved, trust_line_mutation_reserved,
    virtual_tip, funded_trust_line_fixture, CHAIN,
};
use crate::admit::BlockTemplateLanes;

#[test]
fn nodebackend_trust_line_set_reservations_evict_on_block_inclusion() {
    let mut fx = funded_trust_line_fixture();
    let set = signed_trust_line_set(
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        500,
        fx.genesis,
        0,
    );
    fx.backend.submit_drc_trust_line_set(set).unwrap();
    let ast = asset(&fx.issuer, fx.cur_usd);
    assert!(trust_line_mutation_reserved(&fx.backend, &fx.holder.address(), &ast));
    assert!(trust_line_meta_reserved(&fx.backend, &fx.holder.address(), &ast));
    assert!(account_reserved(&fx.backend, &fx.holder.address()));
    mine_template(&mut fx.backend);
    assert!(!trust_line_mutation_reserved(&fx.backend, &fx.holder.address(), &ast));
    assert!(!trust_line_meta_reserved(&fx.backend, &fx.holder.address(), &ast));
    assert!(!account_reserved(&fx.backend, &fx.holder.address()));
}

#[test]
fn nodebackend_reorg_reverts_issue_restores_liability_and_resubmit_once() {
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
    let pool0 = reward_pool_balance(fx.store.as_ref());
    let issue = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        40,
        fx.genesis,
        0,
    );
    let issue_id = issue.issued_transfer_tx_id();
    let fund_tip = virtual_tip(&fx.backend);
    let empty_tip =
        reorg_away_transfer_on_tip(&mut fx.backend, fx.store.as_ref(), fund_tip, &issue);
    assert_eq!(line_balance(fx.store.as_ref(), &fx.holder.address(), &ast), 0);
    assert_eq!(issuer_outstanding(fx.store.as_ref(), &ast), 0);
    assert_eq!(reward_pool_balance(fx.store.as_ref()), pool0);
    assert_liability_equals_sum_balances(fx.store.as_ref(), &fx.issuer.address(), &fx.cur_usd);
    assert!(!fx
        .backend
        .test_mempool()
        .lock()
        .unwrap()
        .contains(&issue_id));

    assert!(fx.backend.submit_drc_issued_transfer(issue.clone()).is_ok());
    assert!(fx.backend.submit_drc_issued_transfer(issue).is_err());
    let _ = empty_tip;
}

#[test]
fn nodebackend_stale_over_limit_transfer_rejected_valid_next_after_reorg() {
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
    let fund_tip = virtual_tip(&fx.backend);
    let first = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        30,
        fx.genesis,
        0,
    );
    reorg_away_transfer_on_tip(&mut fx.backend, fx.store.as_ref(), fund_tip, &first);
    assert_eq!(
        line_balance(fx.store.as_ref(), &fx.holder.address(), &asset(&fx.issuer, fx.cur_usd)),
        0
    );

    let tip = virtual_tip(&fx.backend);
    submit_lanes_at_parents(
        &mut fx.backend,
        &[tip],
        1_000,
        BlockTemplateLanes {
            drc_issued_transfers: std::slice::from_ref(&first),
            ..BlockTemplateLanes::default()
        },
        90,
    );
    assert_eq!(
        line_balance(fx.store.as_ref(), &fx.holder.address(), &asset(&fx.issuer, fx.cur_usd)),
        30
    );

    assert!(fx
        .backend
        .submit_drc_issued_transfer(first.clone())
        .is_err());
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            25,
            fx.genesis,
            1,
        ))
        .is_err());

    let next = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        15,
        fx.genesis,
        1,
    );
    fx.backend.submit_drc_issued_transfer(next.clone()).unwrap();
    let tip = virtual_tip(&fx.backend);
    submit_lanes_at_parents(
        &mut fx.backend,
        &[tip],
        1_000,
        BlockTemplateLanes {
            drc_issued_transfers: std::slice::from_ref(&next),
            ..BlockTemplateLanes::default()
        },
        91,
    );
    assert_eq!(
        line_balance(fx.store.as_ref(), &fx.holder.address(), &asset(&fx.issuer, fx.cur_usd)),
        45
    );
}

#[test]
fn nodebackend_ticket_issue_reserves_consumer_until_inclusion() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        200,
        0,
    );
    let mut ticket = DrcTicketCreateTx::unsigned(fx.issuer.address(), Amount::from_base_units(1), 0);
    sign_drc_ticket_create_bound(&mut ticket, &fx.issuer, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_ticket_create(ticket).unwrap();
    mine_template(&mut fx.backend);

    let ticket_sequence = 1u64;
    let mut issue = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        5,
        fx.genesis,
        0,
    );
    issue.version = DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION;
    issue.account_sequence = Some(DrcAccountSequenceSelector::ticket(ticket_sequence));
    agora_crypto::sign_drc_issued_transfer_bound(&mut issue, &fx.issuer, CHAIN, &fx.genesis)
        .unwrap();
    let issue_id = issue.issued_transfer_tx_id();
    fx.backend.submit_drc_issued_transfer(issue.clone()).unwrap();
    assert!(ticket_consumer_reserved(
        &fx.backend,
        &fx.issuer.address(),
        ticket_sequence
    ));
    let parent = virtual_tip(&fx.backend);
    submit_lanes_at_parents(
        &mut fx.backend,
        &[parent],
        1_000,
        BlockTemplateLanes {
            drc_issued_transfers: std::slice::from_ref(&issue),
            ..BlockTemplateLanes::default()
        },
        45,
    );
    assert!(!ticket_consumer_reserved(
        &fx.backend,
        &fx.issuer.address(),
        ticket_sequence
    ));
    assert!(!fx.backend.test_mempool().lock().unwrap().contains(&issue_id));
    assert!(load_drc_issued_transfer_receipt(fx.store.as_ref(), &issue_id)
        .unwrap()
        .is_some());
}

#[test]
fn nodebackend_reorg_receipt_unknown_until_reapplied() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        500,
        0,
    );
    let xfer = signed_issued_transfer(
        &fx.issuer,
        fx.holder.address(),
        &fx.issuer,
        fx.cur_usd,
        12,
        fx.genesis,
        0,
    );
    let xfer_id = xfer.issued_transfer_tx_id();
    let fund_tip = virtual_tip(&fx.backend);
    reorg_away_transfer_on_tip(&mut fx.backend, fx.store.as_ref(), fund_tip, &xfer);
    assert!(load_drc_issued_transfer_receipt(fx.store.as_ref(), &xfer_id)
        .unwrap()
        .is_none());
    fx.backend.submit_drc_issued_transfer(xfer.clone()).unwrap();
    mine_template(&mut fx.backend);
    assert!(load_drc_issued_transfer_receipt(fx.store.as_ref(), &xfer_id)
        .unwrap()
        .is_some());
}

#[test]
fn nodebackend_rejected_admit_leaves_mempool_and_template_clean() {
    let mut fx = funded_trust_line_fixture();
    setup_live_line(
        &mut fx.backend,
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        10,
        0,
    );
    assert!(fx
        .backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &fx.issuer,
            fx.holder.address(),
            &fx.issuer,
            fx.cur_usd,
            100,
            fx.genesis,
            0,
        ))
        .is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
    let template = fx.backend.get_block_template().unwrap();
    assert!(template.drc_issued_transfers.is_empty());
    assert!(assert_template_has_no_invalid_trust_line_lanes(&template));
}
