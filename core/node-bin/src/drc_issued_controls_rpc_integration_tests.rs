//! RPC + NodeBackend persistence for issued-controls point queries.

use std::sync::{Arc, Mutex};

use agora_crypto::KeyPair;
use agora_rpc::{RpcBackend, RpcDispatcher, RpcRequest};
use agora_state_machine::{credit_account_into, GenesisBuilder, WriteBatch};
use agora_types::{
    Amount, DrcIssuedAssetPolicyAction, IssuedAmount, IssuedCurrencyCode, NativeAssetId,
};
use serde_json::json;

use super::drc_issued_controls_public_helpers::{
    backend_config, boot_chain, mine_template, setup_clawback_ready_line, signed_policy_set,
};
use super::drc_trust_line_public_helpers::{setup_live_line, std_code};

#[test]
fn rpc_policy_set_restart_query_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let holder = KeyPair::from_secret_bytes(&[0xa0; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[0xa2; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let currency = std_code(b"USD");
    let mut funding = WriteBatch::new();
    for kp in [&holder, &issuer] {
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(500_000),
        )
        .unwrap();
    }
    store.write_batch(funding).unwrap();

    let mut backend = crate::backend::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(agora_p2p::Mempool::new(64))),
        backend_config(genesis, miner.address()),
    );
    setup_live_line(&mut backend, genesis, &holder, &issuer, currency, 1_000, 0);
    let policy = signed_policy_set(
        &issuer,
        currency,
        DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        genesis,
        0,
    );
    let policy_id = policy.policy_set_tx_id();
    backend.submit_drc_issued_asset_policy_set(policy).unwrap();
    mine_template(&mut backend);

    let mut dispatcher = RpcDispatcher::new(backend);
    let live = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcIssuedAssetPolicy".into(),
        params: json!({
            "issuer": issuer.address().to_hex(),
            "currency": "USD",
        }),
    });
    assert_eq!(live.result.as_ref().unwrap()["global_freeze"], json!(true));

    let receipt = dispatcher.handle(RpcRequest {
        id: Some(json!(2)),
        method: "agora_getDrcIssuedAssetPolicyReceipt".into(),
        params: json!({ "policy_set_tx_id": policy_id.to_hex() }),
    });
    assert_eq!(receipt.result.as_ref().unwrap()["status"], json!("known"));

    drop(dispatcher);
    drop(store);

    let store2 = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    let backend2 = crate::backend::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store2.clone(), genesis))),
        store2,
        Arc::new(Mutex::new(agora_p2p::Mempool::new(64))),
        backend_config(genesis, miner.address()),
    );
    let mut dispatcher2 = RpcDispatcher::new(backend2);
    let live2 = dispatcher2.handle(RpcRequest {
        id: Some(json!(3)),
        method: "agora_getDrcIssuedAssetPolicy".into(),
        params: json!({
            "issuer": issuer.address().to_hex(),
            "currency": "USD",
        }),
    });
    assert_eq!(live2.result.as_ref().unwrap()["global_freeze"], json!(true));
}

#[test]
fn rpc_clawback_receipt_after_mine() {
    let mut fx = super::drc_issued_controls_public_helpers::funded_trust_line_fixture();
    setup_clawback_ready_line(
        &mut fx.backend,
        fx.store.as_ref(),
        fx.genesis,
        &fx.holder,
        &fx.issuer,
        fx.cur_usd,
        100,
        30,
    );
    let claw = super::drc_issued_controls_public_helpers::signed_clawback(
        &fx.issuer,
        fx.holder.address(),
        fx.cur_usd,
        10,
        fx.genesis,
        2,
    );
    let claw_id = claw.clawback_tx_id();
    fx.backend.submit_drc_issued_clawback(claw).unwrap();
    mine_template(&mut fx.backend);
    let mut dispatcher = RpcDispatcher::new(fx.backend);
    let receipt = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcIssuedClawbackReceipt".into(),
        params: json!({ "clawback_tx_id": claw_id.to_hex() }),
    });
    assert_eq!(receipt.result.as_ref().unwrap()["status"], json!("known"));
}

#[test]
fn rpc_malformed_policy_currency_returns_32602() {
    use agora_types::Address;

    let store = Arc::new(agora_state_machine::StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let backend = crate::backend::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store,
        Arc::new(Mutex::new(agora_p2p::Mempool::new(8))),
        backend_config(genesis, miner.address()),
    );
    let mut dispatcher = RpcDispatcher::new(backend);
    let resp = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcIssuedAssetPolicy".into(),
        params: json!({
            "issuer": Address::ZERO.to_hex(),
            "currency": "usd",
        }),
    });
    assert!(resp.error.is_some());
    assert_eq!(resp.error.as_ref().unwrap().code, -32602);
}
