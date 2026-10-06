//! NodeBackend / public-admission security matrix for DRC payment channels.

use agora_crypto::{
    sign_drc_account_policy_bound, sign_drc_payment_channel_create_bound,
    sign_drc_regular_key_bound, KeyPair,
};
use agora_rpc::RpcBackend;
use agora_state_machine::load_drc_payment_channel_live;
use agora_types::{
    Amount, DrcAccountPolicyTx, DrcMultisignAuth, DrcMultisignEntry, DrcPaymentChannelCloseKind,
    DrcRegularKeyTx, Hash, DRC_MULTISIGN_AUTH_VERSION,
};

use super::drc_payment_channel_public_helpers::{
    account_reserved, assert_conservation_after_fee,
    assert_template_has_no_invalid_payment_channel_lanes, drc_balance, funded_fixture,
    locked_remainder, mempool_len, mine_template, signed_claim, signed_close, signed_create,
    signed_fund, spendable_plus_locked, CHAIN,
};

#[test]
fn public_admission_rejects_malformed_offledger_claim_never_templates() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        100,
        Some(500),
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    let mut bad = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        10,
        0,
        CHAIN,
        &fx.genesis,
    );
    bad.channel_claim_signature = vec![0u8; 8];
    assert!(fx.backend.submit_drc_payment_channel_claim(bad).is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
    let template = fx.backend.get_block_template().unwrap();
    assert!(template.drc_payment_channel_claims.is_empty());
    assert!(assert_template_has_no_invalid_payment_channel_lanes(
        &template
    ));
}

#[test]
fn public_admission_rejects_wrong_offledger_chain_and_genesis() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        50,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    let wrong_genesis = Hash([0xee; 32]);
    let bad = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        5,
        0,
        "wrong-chain",
        &wrong_genesis,
    );
    assert!(fx.backend.submit_drc_payment_channel_claim(bad).is_err());
    assert_eq!(mempool_len(&fx.backend), 0);
}

#[test]
fn public_admission_rejects_equal_lower_excess_cumulative_claims() {
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
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            40,
            0,
            CHAIN,
            &fx.genesis,
        ))
        .unwrap();
    mine_template(&mut fx.backend);

    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            40,
            1,
            CHAIN,
            &fx.genesis,
        ))
        .is_err());
    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            20,
            1,
            CHAIN,
            &fx.genesis,
        ))
        .is_err());
    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            200,
            1,
            CHAIN,
            &fx.genesis,
        ))
        .is_err());
}

#[test]
fn public_admission_rejects_replayed_claim_pending_mutation() {
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
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

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
    fx.backend
        .submit_drc_payment_channel_claim(claim.clone())
        .unwrap();
    assert!(fx.backend.submit_drc_payment_channel_claim(claim).is_err());
    assert!(fx
        .backend
        .test_mempool()
        .lock()
        .unwrap()
        .payment_channel_mutation_reserved(&channel_id));
}

#[test]
fn public_admission_duplicate_pending_channel_mutation_fail_closed() {
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

    fx.backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 10))
        .unwrap();
    assert!(fx
        .backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 2, 5,))
        .is_err());
}

#[test]
fn public_admission_owner_only_fund() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        40,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    let mut bad = signed_fund(&fx.owner, channel_id, fx.genesis, 1, 5);
    bad.submitter = fx.destination.address();
    agora_crypto::sign_drc_payment_channel_fund_bound(
        &mut bad,
        &fx.destination,
        CHAIN,
        &fx.genesis,
    )
    .unwrap();
    assert!(fx.backend.submit_drc_payment_channel_fund(bad).is_err());
}

#[test]
fn public_admission_destination_only_claim() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        40,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    let mut bad = signed_claim(
        &fx.destination,
        &fx.claim_key,
        channel_id,
        fx.genesis,
        5,
        0,
        CHAIN,
        &fx.genesis,
    );
    bad.submitter = fx.owner.address();
    agora_crypto::sign_drc_payment_channel_claim_bound(&mut bad, &fx.owner, CHAIN, &fx.genesis)
        .unwrap();
    assert!(fx.backend.submit_drc_payment_channel_claim(bad).is_err());
}

#[test]
fn public_admission_destination_claim_allowed_under_deposit_auth() {
    let mut fx = funded_fixture();
    let mut pol = DrcAccountPolicyTx::set_deposit_auth_required(
        fx.destination.address(),
        Amount::from_base_units(1),
        0,
    );
    sign_drc_account_policy_bound(&mut pol, &fx.destination, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_account_policy(pol).unwrap();
    mine_template(&mut fx.backend);

    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        50,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);

    assert!(fx
        .backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            10,
            1,
            CHAIN,
            &fx.genesis,
        ))
        .is_ok());
}

#[test]
fn public_admission_rejects_onchain_claim_at_cancel_after_blue_score() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        50,
        Some(3),
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);
    for _ in 0..4 {
        mine_template(&mut fx.backend);
    }

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
}

#[test]
fn public_admission_disabled_master_rejected_regular_key_succeeds() {
    let mut fx = funded_fixture();
    let regular = KeyPair::from_secret_bytes(&[0x61; 32]).unwrap();
    let mut reg = DrcRegularKeyTx::set(
        fx.owner.address(),
        regular.address(),
        regular.public_key_bytes().to_vec(),
        Amount::from_base_units(1),
        0,
    );
    sign_drc_regular_key_bound(&mut reg, &fx.owner, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_regular_key(reg).unwrap();
    mine_template(&mut fx.backend);

    let mut disable = DrcAccountPolicyTx::set_master_key_disabled(
        fx.owner.address(),
        Amount::from_base_units(1),
        1,
    );
    sign_drc_account_policy_bound(&mut disable, &fx.owner, CHAIN, &fx.genesis).unwrap();
    fx.backend.submit_drc_account_policy(disable).unwrap();
    mine_template(&mut fx.backend);

    let mut create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        2,
        30,
        None,
    );
    assert!(fx
        .backend
        .submit_drc_payment_channel_create(create.clone())
        .is_err());
    sign_drc_payment_channel_create_bound(&mut create, &regular, CHAIN, &fx.genesis).unwrap();
    assert!(fx.backend.submit_drc_payment_channel_create(create).is_ok());
}

#[test]
fn public_admission_nonce_reservation_released_after_reject() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        10_000_000,
        None,
    );
    assert!(fx
        .backend
        .submit_drc_payment_channel_create(create)
        .is_err());
    assert!(!account_reserved(&fx.backend, &fx.owner.address()));
}

#[test]
fn public_admission_pending_create_fail_closed_for_dependents() {
    let mut fx = funded_fixture();
    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        50,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    assert!(fx
        .backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 5,))
        .is_err());
}

#[test]
fn public_template_multisign_missing_attachment_never_admitted() {
    let mut fx = funded_fixture();
    let mut create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        20,
        None,
    );
    create.multisign = Some(DrcMultisignAuth {
        version: DRC_MULTISIGN_AUTH_VERSION,
        signing_for: fx.owner.address(),
        signatures: vec![DrcMultisignEntry {
            signer: fx.owner.address(),
            public_key: fx.owner.public_key_bytes().to_vec(),
            signature: vec![0u8; 64],
        }],
    });
    assert!(fx
        .backend
        .submit_drc_payment_channel_create(create)
        .is_err());
    let template = fx.backend.get_block_template().unwrap();
    assert!(template.drc_payment_channel_creates.is_empty());
}

#[test]
fn public_exact_conservation_create_fund_two_claims_schedule_finalize() {
    let mut fx = funded_fixture();
    let owner = fx.owner.address();
    let dest = fx.destination.address();
    let owner0 = drc_balance(fx.store.as_ref(), &owner);
    let dest0 = drc_balance(fx.store.as_ref(), &dest);
    let mut total = spendable_plus_locked(&fx.store, &owner, &dest, &Hash::ZERO);

    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        200,
        Some(800),
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);
    let after_create = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, after_create, 1);
    total = after_create;

    fx.backend
        .submit_drc_payment_channel_fund(signed_fund(&fx.owner, channel_id, fx.genesis, 1, 50))
        .unwrap();
    mine_template(&mut fx.backend);
    let after_fund = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, after_fund, 1);
    total = after_fund;

    mine_template(&mut fx.backend);
    let mut dest_before = drc_balance(fx.store.as_ref(), &dest);
    let mut locked_before = locked_remainder(fx.store.as_ref(), &channel_id);
    fx.backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            30,
            0,
            CHAIN,
            &fx.genesis,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    assert_eq!(
        drc_balance(fx.store.as_ref(), &dest),
        dest_before + 30 - 1,
        "destination credit minus claim fee"
    );
    assert_eq!(
        locked_remainder(fx.store.as_ref(), &channel_id),
        locked_before - 30
    );
    let after_claim1 = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, after_claim1, 1);
    total = after_claim1;

    dest_before = drc_balance(fx.store.as_ref(), &dest);
    locked_before = locked_remainder(fx.store.as_ref(), &channel_id);
    fx.backend
        .submit_drc_payment_channel_claim(signed_claim(
            &fx.destination,
            &fx.claim_key,
            channel_id,
            fx.genesis,
            55,
            1,
            CHAIN,
            &fx.genesis,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    assert_eq!(drc_balance(fx.store.as_ref(), &dest), dest_before + 25 - 1);
    assert_eq!(
        locked_remainder(fx.store.as_ref(), &channel_id),
        locked_before - 25
    );
    let after_claim2 = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, after_claim2, 1);
    total = after_claim2;

    fx.backend
        .submit_drc_payment_channel_close(signed_close(
            &fx.owner,
            channel_id,
            fx.genesis,
            DrcPaymentChannelCloseKind::OwnerScheduleClose,
            2,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    let after_schedule = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, after_schedule, 1);
    total = after_schedule;

    for _ in 0..8 {
        mine_template(&mut fx.backend);
    }
    fx.backend
        .submit_drc_payment_channel_close(signed_close(
            &fx.owner,
            channel_id,
            fx.genesis,
            DrcPaymentChannelCloseKind::Finalize,
            3,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    let after_finalize = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, after_finalize, 1);
    assert!(
        load_drc_payment_channel_live(fx.store.as_ref(), &channel_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        drc_balance(fx.store.as_ref(), &dest),
        dest0 + 53,
        "destination receives cumulative 55 minus two claim fees"
    );
    assert_eq!(
        drc_balance(fx.store.as_ref(), &owner) + drc_balance(fx.store.as_ref(), &dest),
        owner0 + dest0 - 6,
        "closed channel: owner+destination spendable differs from initial only by lane fees"
    );
}

#[test]
fn public_exact_conservation_destination_immediate_close() {
    let mut fx = funded_fixture();
    let owner = fx.owner.address();
    let dest = fx.destination.address();
    let mut total = spendable_plus_locked(&fx.store, &owner, &dest, &Hash::ZERO);

    let create = signed_create(
        &fx.owner,
        &fx.claim_key,
        fx.destination.address(),
        fx.genesis,
        0,
        120,
        None,
    );
    let channel_id = create.channel_id();
    fx.backend
        .submit_drc_payment_channel_create(create)
        .unwrap();
    mine_template(&mut fx.backend);
    let t1 = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, t1, 1);
    total = t1;

    fx.backend
        .submit_drc_payment_channel_close(signed_close(
            &fx.destination,
            channel_id,
            fx.genesis,
            DrcPaymentChannelCloseKind::DestinationClose,
            0,
        ))
        .unwrap();
    mine_template(&mut fx.backend);
    let t2 = spendable_plus_locked(&fx.store, &owner, &dest, &channel_id);
    assert_conservation_after_fee(total, t2, 1);
    assert!(
        load_drc_payment_channel_live(fx.store.as_ref(), &channel_id)
            .unwrap()
            .is_none()
    );
}
