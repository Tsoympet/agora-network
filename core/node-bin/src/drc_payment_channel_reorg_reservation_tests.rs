//! NodeBackend payment-channel reorg, reservation eviction, and post-accounting regression.

#![allow(unused_assignments, unused_variables, unused_imports)]

use agora_crypto::{sign_drc_payment_channel_fund_bound, sign_drc_ticket_create_bound, KeyPair};
use agora_rpc::RpcBackend;
use agora_state_machine::load_drc_payment_channel_live;
use agora_types::{
    DrcAccountSequenceSelector, DrcPaymentChannelCloseKind, DrcTicketCreateTx, Hash,
    DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
};

use super::drc_payment_channel_public_helpers::{
    account_reserved, assert_template_has_no_invalid_payment_channel_lanes,
    channel_conservation_quad, drc_balance, funded_fixture, live_cumulative_claimed,
    locked_remainder, mine_template, payment_channel_mutation_reserved,
    reorg_away_claim_on_fund_tip, signed_claim, signed_close, signed_create, signed_fund,
    spendable_plus_locked, submit_lanes_at_parents, ticket_consumer_reserved, virtual_tip, CHAIN,
};
use crate::admit::BlockTemplateLanes;

#[test]
fn nodebackend_payment_channel_nonce_mutations_reserve_until_block_eviction() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        80,
        None,
    );
    let channel_id = create.channel_id();
    let create_id = fx
        .backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    assert!(account_reserved(&fx.backend, &fx.owner.address()));
    {
        let pool = fx.backend.test_mempool().lock().unwrap();
        assert!(pool.pending_payment_channel_create(&channel_id));
        assert!(pool.contains(&create_id));
    }
    mine_template(&mut fx.backend);
    assert!(!account_reserved(&fx.backend, &fx.owner.address()));
    assert!(!payment_channel_mutation_reserved(&fx.backend, &channel_id));

    let fund = signed_fund(&fx.owner, channel_id, fx.genesis, 1, 20);
    let fund_id = fx.backend.submit_drc_payment_channel_fund(fund).unwrap();
    assert!(account_reserved(&fx.backend, &fx.owner.address()));
    assert!(payment_channel_mutation_reserved(&fx.backend, &channel_id));
    mine_template(&mut fx.backend);
    assert!(!account_reserved(&fx.backend, &fx.owner.address()));
    assert!(!payment_channel_mutation_reserved(&fx.backend, &channel_id));
    assert!(!fx.backend.test_mempool().lock().unwrap().contains(&fund_id));

    let claim = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        15,
        0,
        CHAIN,
        &fx.genesis,
    );
    let claim_id = fx.backend.submit_drc_payment_channel_claim(claim).unwrap();
    assert!(account_reserved(&fx.backend, &fx.destination.address()));
    assert!(payment_channel_mutation_reserved(&fx.backend, &channel_id));
    mine_template(&mut fx.backend);
    assert!(!account_reserved(&fx.backend, &fx.destination.address()));
    assert!(!payment_channel_mutation_reserved(&fx.backend, &channel_id));
    assert!(!fx
        .backend
        .test_mempool()
        .lock()
        .unwrap()
        .contains(&claim_id));

    let close = signed_close(
        &fx.owner,
        channel_id,
        fx.genesis,
        DrcPaymentChannelCloseKind::OwnerScheduleClose,
        2,
    );
    let close_id = fx.backend.submit_drc_payment_channel_close(close).unwrap();
    assert!(account_reserved(&fx.backend, &fx.owner.address()));
    assert!(payment_channel_mutation_reserved(&fx.backend, &channel_id));
    mine_template(&mut fx.backend);
    assert!(!account_reserved(&fx.backend, &fx.owner.address()));
    assert!(!payment_channel_mutation_reserved(&fx.backend, &channel_id));
    assert!(!fx
        .backend
        .test_mempool()
        .lock()
        .unwrap()
        .contains(&close_id));
}

#[test]
fn nodebackend_payment_channel_ticket_fund_reserves_consumer_until_block_eviction() {
    use agora_types::Amount;

    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        60,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    let mut ticket = DrcTicketCreateTx::unsigned(fx.owner.address(), Amount::from_base_units(1), 1);
    sign_drc_ticket_create_bound(&mut ticket, &fx.owner, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_ticket_create(ticket).unwrap();
    mine_template(&mut fx.backend);

    let ticket_sequence = 2u64;
    let mut fund = agora_types::DrcPaymentChannelFundTx {
        version: DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
        submitter: fx.owner.address(),
        channel_id,
        amount: Amount::from_base_units(10),
        fee: Amount::from_base_units(1),
        nonce: 0,
        account_sequence: Some(DrcAccountSequenceSelector::ticket(ticket_sequence)),
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_payment_channel_fund_bound(&mut fund, &fx.owner, CHAIN, &fx.genesis).unwrap();
    let fund_id = fx
        .backend
        .submit_drc_payment_channel_fund(fund.clone())
        .unwrap();
    assert!(ticket_consumer_reserved(
        &fx.backend,
        &fx.owner.address(),
        ticket_sequence
    ));
    assert!(payment_channel_mutation_reserved(&fx.backend, &channel_id));
    assert!(!account_reserved(&fx.backend, &fx.owner.address()));

    let lanes = BlockTemplateLanes {
        drc_payment_channel_funds: std::slice::from_ref(&fund),
        ..BlockTemplateLanes::default()
    };
    let parent = virtual_tip(&fx.backend);
    submit_lanes_at_parents(&mut fx.backend, &[parent], 1_000, lanes, 44);
    assert!(!ticket_consumer_reserved(
        &fx.backend,
        &fx.owner.address(),
        ticket_sequence
    ));
    assert!(!payment_channel_mutation_reserved(&fx.backend, &channel_id));
    assert!(!fx.backend.test_mempool().lock().unwrap().contains(&fund_id));
}

#[test]
fn nodebackend_reorg_reverts_claim_restores_canonical_and_resubmit_once() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        100,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 40))
        .unwrap();
    mine_template(&mut fx.backend);
    let fund_tip = virtual_tip(&fx.backend);
    assert_eq!(live_cumulative_claimed(fx.store.as_ref(), &channel_id), 0);

    let claim = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        25,
        0,
        CHAIN,
        &fx.genesis,
    );
    let claim_id = claim.claim_tx_id();
    let empty_tip =
        reorg_away_claim_on_fund_tip(&mut fx.backend, fx.store.as_ref(), fund_tip, &claim);
    assert_eq!(virtual_tip(&fx.backend), empty_tip);
    assert!(!fx
        .backend
        .test_mempool()
        .lock()
        .unwrap()
        .contains(&claim_id));

    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(claim.clone())
        .is_ok());
    assert!(fx.backend.submit_drc_payment_channel_claim(claim).is_err());
    let _ = empty_tip;
}

#[test]
fn nodebackend_stale_claims_rejected_valid_next_accepted_after_reorg() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        90,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 50))
        .unwrap();
    mine_template(&mut fx.backend);
    let fund_tip = virtual_tip(&fx.backend);

    let first = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        20,
        0,
        CHAIN,
        &fx.genesis,
    );
    reorg_away_claim_on_fund_tip(&mut fx.backend, fx.store.as_ref(), fund_tip, &first);
    assert_eq!(live_cumulative_claimed(fx.store.as_ref(), &channel_id), 0);

    let tip = virtual_tip(&fx.backend);
    submit_lanes_at_parents(
        &mut fx.backend,
        &[tip],
        1_000,
        BlockTemplateLanes {
            drc_payment_channel_claims: std::slice::from_ref(&first),
            ..BlockTemplateLanes::default()
        },
        80,
    );
    assert_eq!(live_cumulative_claimed(fx.store.as_ref(), &channel_id), 20);

    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(first.clone())
        .is_err());
    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            15,
            1,
            CHAIN,
            &fx.genesis,
        ))
        .is_err());

    let next = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        35,
        1,
        CHAIN,
        &fx.genesis,
    );
    fx.backend
        .submit_drc_payment_channel_claim(next.clone())
        .unwrap();
    let tip = virtual_tip(&fx.backend);
    submit_lanes_at_parents(
        &mut fx.backend,
        &[tip],
        1_000,
        BlockTemplateLanes {
            drc_payment_channel_claims: std::slice::from_ref(&next),
            ..BlockTemplateLanes::default()
        },
        81,
    );
    assert_eq!(live_cumulative_claimed(fx.store.as_ref(), &channel_id), 35);
}

#[test]
fn nodebackend_pending_channel_mutation_conflicts_fail_closed_through_revalidation() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        70,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    fx.backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 10))
        .unwrap();
    assert!(payment_channel_mutation_reserved(&fx.backend, &channel_id));
    assert!(fx
        .backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 2, 5,))
        .is_err());
    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            5,
            0,
            CHAIN,
            &fx.genesis,
        ))
        .is_err());

    let template = fx.backend.get_block_template().unwrap();
    assert_eq!(template.drc_payment_channel_funds.len(), 1);
    assert!(assert_template_has_no_invalid_payment_channel_lanes(
        &template
    ));
    assert!(template.drc_payment_channel_claims.is_empty());
}

#[test]
fn nodebackend_block_template_never_emits_invalid_payment_channel_lanes() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        45,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 12))
        .unwrap();
    mine_template(&mut fx.backend);
    fx.backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            10,
            0,
            CHAIN,
            &fx.genesis,
        ))
        .unwrap();
    let template = fx.backend.get_block_template().unwrap();
    assert!(assert_template_has_no_invalid_payment_channel_lanes(
        &template
    ));
    assert_eq!(template.header.tx_root, template.compute_body_root());
}

#[cfg(feature = "rocksdb")]
#[test]
fn nodebackend_post_accounting_claim_revert_reapply_restart_conservation() {
    use agora_state_machine::{GenesisBuilder, StateStore, WriteBatch};
    use agora_types::{Amount, NativeAssetId};
    use std::sync::{Arc, Mutex};

    use super::drc_payment_channel_public_helpers::{backend_config, boot_chain};
    use agora_p2p::Mempool;

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x81; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[0x82; 32]).unwrap();
    let claim_key = KeyPair::from_secret_bytes(&[0x83; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x84; 32]).unwrap();
    let mut funding = WriteBatch::new();
    agora_state_machine::credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(500_000),
    )
    .unwrap();
    agora_state_machine::credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &destination.address(),
        Amount::from_base_units(50_000),
    )
    .unwrap();
    agora_state_machine::put_issued_supply_into(&mut funding, NativeAssetId::DRC, 550_000);
    store.write_batch(funding).unwrap();

    let mut backend = crate::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(Mempool::new(64))),
        backend_config(genesis, miner.address()),
    );

    let owner_addr = owner.address();
    let dest_addr = destination.address();
    let quad0 = channel_conservation_quad(store.as_ref(), &owner_addr, &dest_addr, &Hash::ZERO);
    let total0: u64 = quad0.0 + quad0.1 + quad0.2 + quad0.3;
    let initial_spendable =
        spendable_plus_locked(store.as_ref(), &owner_addr, &dest_addr, &Hash::ZERO);

    let create = signed_create(&owner, &claim_key, dest_addr, genesis, 0, 150, Some(900));
    let channel_id = create.channel_id();
    backend.submit_drc_payment_channel_create(create).unwrap();
    mine_template(&mut backend);
    let quad1 = channel_conservation_quad(store.as_ref(), &owner_addr, &dest_addr, &channel_id);
    assert_eq!(quad1.0 + quad1.1 + quad1.2 + quad1.3, total0);
    assert_eq!(
        spendable_plus_locked(store.as_ref(), &owner_addr, &dest_addr, &channel_id),
        initial_spendable - 1
    );

    backend
        .submit_drc_payment_channel_fund(signed_fund(&owner, channel_id, genesis, 1, 60))
        .unwrap();
    mine_template(&mut backend);
    let fund_tip = virtual_tip(&backend);
    let quad2 = channel_conservation_quad(store.as_ref(), &owner_addr, &dest_addr, &channel_id);
    let burned_after_fund = quad2.3;
    assert_eq!(quad2.0 + quad2.1 + quad2.2 + quad2.3, total0);
    assert_eq!(
        spendable_plus_locked(store.as_ref(), &owner_addr, &dest_addr, &channel_id),
        initial_spendable - 2
    );

    let claim = signed_claim(
        &destination,
        &claim_key,
        channel_id,
        genesis,
        40,
        0,
        CHAIN,
        &genesis,
    );
    submit_lanes_at_parents(
        &mut backend,
        &[fund_tip],
        1_000,
        BlockTemplateLanes {
            drc_payment_channel_claims: std::slice::from_ref(&claim),
            ..BlockTemplateLanes::default()
        },
        71,
    );
    let dest_after_claim = drc_balance(store.as_ref(), &dest_addr);
    let locked_after_claim = locked_remainder(store.as_ref(), &channel_id);
    assert_eq!(dest_after_claim, quad0.1 + 40 - 1);
    assert_eq!(locked_after_claim, 150 + 60 - 40);
    let quad3 = channel_conservation_quad(store.as_ref(), &owner_addr, &dest_addr, &channel_id);
    assert_eq!(quad3.0 + quad3.1 + quad3.2 + quad3.3, total0);
    assert_eq!(
        spendable_plus_locked(store.as_ref(), &owner_addr, &dest_addr, &channel_id),
        initial_spendable - 3
    );

    reorg_away_claim_on_fund_tip(&mut backend, store.as_ref(), fund_tip, &claim);
    let quad_revert =
        channel_conservation_quad(store.as_ref(), &owner_addr, &dest_addr, &channel_id);
    assert_eq!(quad_revert.1, quad0.1);
    assert_eq!(quad_revert.2, 150 + 60);
    assert_eq!(quad_revert.3, burned_after_fund);
    assert_eq!(
        quad_revert.0 + quad_revert.1 + quad_revert.2 + quad_revert.3,
        total0
    );
    assert_eq!(
        spendable_plus_locked(store.as_ref(), &owner_addr, &dest_addr, &channel_id),
        initial_spendable - 2
    );

    assert!(backend
        .submit_drc_payment_channel_claim(claim.clone())
        .is_ok());
    assert!(backend.submit_drc_payment_channel_claim(claim).is_err());

    let path = dir.path().to_path_buf();
    drop(backend);
    drop(store);

    let reopened = StateStore::open(&path).unwrap();
    assert_eq!(
        live_cumulative_claimed(&reopened, &channel_id),
        0,
        "reverted channel state must persist across restart"
    );
    let quad_restart = channel_conservation_quad(&reopened, &owner_addr, &dest_addr, &channel_id);
    assert_eq!(quad_restart, quad_revert);
    assert_eq!(
        spendable_plus_locked(&reopened, &owner_addr, &dest_addr, &channel_id),
        initial_spendable - 2
    );
}
