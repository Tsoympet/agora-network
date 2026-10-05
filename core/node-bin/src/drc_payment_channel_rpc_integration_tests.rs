//! RPC dispatch + NodeBackend persistence for payment channel queries (non-finality).

use std::sync::{Arc, Mutex};

use agora_crypto::{sign_payment_channel_offledger_claim, KeyPair};
use agora_rpc::{InMemoryBackend, RpcBackend, RpcDispatcher, RpcRequest};
use agora_state_machine::{
    credit_account_into, load_drc_payment_channel_fund_event, lookup_drc_payment_channel_point,
    GenesisBuilder, WriteBatch,
};
use agora_types::{Amount, Hash, NativeAssetId};
use serde_json::json;

use super::drc_payment_channel_public_helpers::{
    backend_config, boot_chain, mine_template, signed_create, signed_fund, CHAIN,
};
use crate::backend::NodeBackend;

#[test]
fn rpc_point_queries_and_verify_after_restart_not_finality() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let owner = KeyPair::from_secret_bytes(&[0x80; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[0x81; 32]).unwrap();
    let claim_key = KeyPair::from_secret_bytes(&[0x82; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(500_000),
    )
    .unwrap();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &destination.address(),
        Amount::from_base_units(50_000),
    )
    .unwrap();
    store.write_batch(funding).unwrap();

    let mut backend = NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store.clone(),
        Arc::new(Mutex::new(agora_p2p::Mempool::new(32))),
        backend_config(genesis, miner.address()),
    );

    let create = signed_create(
        &owner,
        &claim_key,
        destination.address(),
        genesis,
        0,
        80,
        None,
    );
    let channel_id = create.channel_id();
    backend.submit_drc_payment_channel_create(create).unwrap();
    mine_template(&mut backend);
    let fund = signed_fund(&owner, channel_id, genesis, 1, 10);
    let fund_id = fund.fund_tx_id();
    backend.submit_drc_payment_channel_fund(fund).unwrap();
    mine_template(&mut backend);

    let mut dispatcher = RpcDispatcher::new(backend);
    let live = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcPaymentChannel".into(),
        params: json!({ "channel_id": channel_id.to_hex() }),
    });
    assert_eq!(live.result.as_ref().unwrap()["status"], json!("live"));
    assert!(live.error.is_none());

    let fund_ev = dispatcher.handle(RpcRequest {
        id: Some(json!(2)),
        method: "agora_getDrcPaymentChannelFundEvent".into(),
        params: json!({ "fund_tx_id": fund_id.to_hex() }),
    });
    assert_eq!(fund_ev.result.as_ref().unwrap()["status"], json!("known"));

    let sig = sign_payment_channel_offledger_claim(
        &claim_key,
        CHAIN,
        &genesis,
        &channel_id,
        Amount::from_base_units(5),
    )
    .unwrap();
    let sig_hex: String = sig.iter().map(|b| format!("{:02x}", b)).collect();
    let verify = dispatcher.handle(RpcRequest {
        id: Some(json!(3)),
        method: "agora_verifyDrcPaymentChannelClaim".into(),
        params: json!({
            "channel_id": channel_id.to_hex(),
            "cumulative_authorized": "5",
            "channel_claim_signature": sig_hex,
        }),
    });
    assert_eq!(verify.result.as_ref().unwrap()["valid"], json!(true));

    let bad = dispatcher.handle(RpcRequest {
        id: Some(json!(4)),
        method: "agora_getDrcPaymentChannel".into(),
        params: json!({ "channel_id": "not-hex" }),
    });
    assert_eq!(bad.error.as_ref().unwrap().code, -32602);

    drop(dispatcher);
    drop(store);

    let store2 = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    assert_eq!(
        lookup_drc_payment_channel_point(store2.as_ref(), &channel_id).unwrap(),
        "live"
    );
    assert!(
        load_drc_payment_channel_fund_event(store2.as_ref(), &fund_id)
            .unwrap()
            .is_some()
    );
}

#[test]
fn rpc_verify_rejects_bad_signature_without_private_material() {
    let mut fx = super::drc_payment_channel_public_helpers::funded_fixture();
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

    let mut dispatcher = RpcDispatcher::new(fx.backend);
    let bad_sig = dispatcher.handle(RpcRequest {
        id: Some(json!(10)),
        method: "agora_verifyDrcPaymentChannelClaim".into(),
        params: json!({
            "channel_id": channel_id.to_hex(),
            "cumulative_authorized": "1",
            "channel_claim_signature": "deadbeef",
        }),
    });
    assert!(bad_sig.error.is_some());
    assert_ne!(
        bad_sig.result.as_ref().and_then(|v| v.get("valid")),
        Some(&json!(true))
    );

    let mut mem_rpc = RpcDispatcher::new(InMemoryBackend::new());
    let unknown_channel = mem_rpc.handle(RpcRequest {
        id: Some(json!(11)),
        method: "agora_verifyDrcPaymentChannelClaim".into(),
        params: json!({
            "channel_id": Hash([7; 32]).to_hex(),
            "cumulative_authorized": "1",
            "channel_claim_signature": format!("{}", "a".repeat(128)),
        }),
    });
    assert!(unknown_channel.error.is_some());
}
