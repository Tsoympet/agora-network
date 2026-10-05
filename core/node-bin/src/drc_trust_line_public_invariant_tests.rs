//! Multi-holder, multi-asset liability invariant through reorg and restart.

use std::sync::{Arc, Mutex};

use agora_crypto::KeyPair;
use agora_rpc::{RpcBackend, RpcDispatcher, RpcRequest};
use agora_state_machine::{credit_account_into, GenesisBuilder, WriteBatch};
use agora_types::{Amount, NativeAssetId};
use serde_json::json;

use super::drc_trust_line_public_helpers::{
    assert_liability_equals_sum_balances, backend_config, boot_chain, drc_balance,
    mine_template, reward_pool_balance, setup_live_line, signed_issued_transfer,
    signed_trust_line_set,
};

fn snapshot_native_totals(
    store: &agora_state_machine::StateStore,
    addrs: &[agora_types::Address],
) -> (u64, u64) {
    let mut spendable = 0u64;
    for a in addrs {
        spendable += drc_balance(store, a);
    }
    (spendable, reward_pool_balance(store))
}

#[test]
fn public_invariant_two_assets_multi_holder_reorg_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let holder = KeyPair::from_secret_bytes(&[0xa0; 32]).unwrap();
    let holder_b = KeyPair::from_secret_bytes(&[0xa1; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[0xa2; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let cur_usd = super::drc_trust_line_public_helpers::std_code(b"USD");
    let cur_eur = super::drc_trust_line_public_helpers::std_code(b"EUR");

    let mut funding = WriteBatch::new();
    for kp in [&holder, &holder_b, &issuer] {
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(5_000_000),
        )
        .unwrap();
    }
    store.write_batch(funding).unwrap();

    let mut backend = super::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(agora_p2p::Mempool::new(128))),
        backend_config(genesis, miner.address()),
    );

    let addrs = [
        holder.address(),
        holder_b.address(),
        issuer.address(),
        miner.address(),
    ];
    let (native0, pool0) = snapshot_native_totals(store.as_ref(), &addrs);

    setup_live_line(&mut backend, genesis, &holder, &issuer, cur_usd, 1_000, 0);
    setup_live_line(&mut backend, genesis, &holder_b, &issuer, cur_eur, 2_000, 0);
    assert_liability_equals_sum_balances(store.as_ref(), &issuer.address(), &cur_usd);
    assert_liability_equals_sum_balances(store.as_ref(), &issuer.address(), &cur_eur);

    backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &issuer,
            holder.address(),
            &issuer,
            cur_usd,
            100,
            genesis,
            0,
        ))
        .unwrap();
    mine_template(&mut backend);
    backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &issuer,
            holder_b.address(),
            &issuer,
            cur_eur,
            200,
            genesis,
            1,
        ))
        .unwrap();
    mine_template(&mut backend);

    setup_live_line(&mut backend, genesis, &holder_b, &issuer, cur_usd, 500, 1);
    backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &holder,
            holder_b.address(),
            &issuer,
            cur_usd,
            30,
            genesis,
            1,
        ))
        .unwrap();
    mine_template(&mut backend);
    assert_liability_equals_sum_balances(store.as_ref(), &issuer.address(), &cur_usd);

    assert!(backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &holder,
            issuer.address(),
            &issuer,
            cur_usd,
            35,
            genesis,
            2,
        ))
        .is_ok());
    mine_template(&mut backend);
    backend
        .submit_drc_issued_transfer(signed_issued_transfer(
            &holder,
            issuer.address(),
            &issuer,
            cur_usd,
            35,
            genesis,
            3,
        ))
        .unwrap();
    mine_template(&mut backend);
    backend
        .submit_drc_trust_line_set(signed_trust_line_set(
            &holder,
            &issuer,
            cur_usd,
            0,
            genesis,
            4,
        ))
        .unwrap();
    mine_template(&mut backend);
    assert_liability_equals_sum_balances(store.as_ref(), &issuer.address(), &cur_usd);

    let (native1, pool1) = snapshot_native_totals(store.as_ref(), &addrs);
    assert_eq!(
        native1 + pool1,
        native0 + pool0,
        "native DRC + reward pool conserved"
    );

    drop(backend);
    drop(store);
    let store2 = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    let backend2 = super::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store2.clone(), genesis))),
        store2.clone(),
        Arc::new(Mutex::new(agora_p2p::Mempool::new(64))),
        backend_config(genesis, miner.address()),
    );
    let mut dispatcher = RpcDispatcher::new(backend2);
    let line = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcTrustLine".into(),
        params: json!({
            "holder": holder_b.address().to_hex(),
            "issuer": issuer.address().to_hex(),
            "currency": "EUR",
        }),
    });
    assert_eq!(line.result.as_ref().unwrap()["balance"], json!("200"));
    assert_liability_equals_sum_balances(store2.as_ref(), &issuer.address(), &cur_eur);
}
