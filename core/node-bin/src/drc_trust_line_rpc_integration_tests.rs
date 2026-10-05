//! RPC + NodeBackend persistence for trust line point queries.

use std::sync::{Arc, Mutex};

use agora_crypto::{sign_drc_issued_transfer_bound, sign_drc_trust_line_set_bound, KeyPair};
use agora_rpc::{RpcBackend, RpcDispatcher, RpcRequest};
use agora_state_machine::{credit_account_into, GenesisBuilder, WriteBatch};
use agora_types::{
    Amount, Hash, IssuedAmount, IssuedCurrencyCode, NativeAssetId,
    DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION, DRC_TRUST_LINE_SET_TX_VERSION,
};
use serde_json::json;

use super::drc_payment_channel_public_helpers::{backend_config, boot_chain, mine_template, CHAIN};

#[test]
fn rpc_trust_line_create_issue_restart_query() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(agora_state_machine::StateStore::open(dir.path()).unwrap());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let holder = KeyPair::from_secret_bytes(&[0x90; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[0x91; 32]).unwrap();
    let recipient = KeyPair::from_secret_bytes(&[0x92; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let currency = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");

    let mut funding = WriteBatch::new();
    for kp in [&holder, &issuer, &recipient] {
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

    let mut set = agora_types::DrcTrustLineSetTx {
        version: DRC_TRUST_LINE_SET_TX_VERSION,
        holder: recipient.address(),
        issuer: issuer.address(),
        currency,
        limit: IssuedAmount::from_units(1_000),
        fee: Amount::from_base_units(1),
        nonce: 0,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_trust_line_set_bound(&mut set, &recipient, CHAIN, &genesis).unwrap();
    backend.submit_drc_trust_line_set(set).unwrap();
    mine_template(&mut backend);

    let mut issue = agora_types::DrcIssuedTransferTx {
        version: DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
        sender: issuer.address(),
        recipient: recipient.address(),
        issuer: issuer.address(),
        currency,
        amount: IssuedAmount::from_units(100),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        nonce: 0,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_issued_transfer_bound(&mut issue, &issuer, CHAIN, &genesis).unwrap();
    let transfer_id = issue.issued_transfer_tx_id();
    backend.submit_drc_issued_transfer(issue).unwrap();
    mine_template(&mut backend);

    let mut dispatcher = RpcDispatcher::new(backend);
    let line = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcTrustLine".into(),
        params: json!({
            "holder": recipient.address().to_hex(),
            "issuer": issuer.address().to_hex(),
            "currency": "USD",
        }),
    });
    assert_eq!(line.result.as_ref().unwrap()["status"], json!("live"));
    assert_eq!(line.result.as_ref().unwrap()["balance"], json!("100"));

    let liability = dispatcher.handle(RpcRequest {
        id: Some(json!(2)),
        method: "agora_getDrcIssuerLiability".into(),
        params: json!({
            "issuer": issuer.address().to_hex(),
            "currency": "USD",
        }),
    });
    assert_eq!(liability.result.as_ref().unwrap()["status"], json!("live"));
    assert_eq!(liability.result.as_ref().unwrap()["outstanding"], json!("100"));

    let receipt = dispatcher.handle(RpcRequest {
        id: Some(json!(3)),
        method: "agora_getDrcIssuedTransferReceipt".into(),
        params: json!({ "transfer_tx_id": transfer_id.to_hex() }),
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
    let line2 = dispatcher2.handle(RpcRequest {
        id: Some(json!(4)),
        method: "agora_getDrcTrustLine".into(),
        params: json!({
            "holder": recipient.address().to_hex(),
            "issuer": issuer.address().to_hex(),
            "currency": "USD",
        }),
    });
    assert_eq!(line2.result.as_ref().unwrap()["balance"], json!("100"));
}

#[test]
fn rpc_malformed_currency_returns_32602() {
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
    let bad = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcTrustLine".into(),
        params: json!({
            "holder": Address::ZERO.to_hex(),
            "issuer": Address::ZERO.to_hex(),
            "currency": "usd",
        }),
    });
    assert!(bad.error.is_some());
    assert_eq!(bad.error.as_ref().unwrap().code, -32602);
}

#[test]
fn rpc_positional_currency_array_submit_and_query() {
    let store = Arc::new(agora_state_machine::StateStore::open_in_memory());
    let genesis = GenesisBuilder::default().ignite(&store).unwrap();
    let holder = KeyPair::from_secret_bytes(&[0x40; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[0x41; 32]).unwrap();
    let miner = KeyPair::from_secret_bytes(&[0x99; 32]).unwrap();
    let mut funding = WriteBatch::new();
    for kp in [&holder, &issuer] {
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &kp.address(),
            Amount::from_base_units(100_000),
        )
        .unwrap();
    }
    store.write_batch(funding).unwrap();
    let mut backend = crate::backend::NodeBackend::new(
        Arc::new(Mutex::new(boot_chain(store.clone(), genesis))),
        store,
        Arc::new(Mutex::new(agora_p2p::Mempool::new(32))),
        backend_config(genesis, miner.address()),
    );
    let mut set = agora_types::DrcTrustLineSetTx {
        version: DRC_TRUST_LINE_SET_TX_VERSION,
        holder: holder.address(),
        issuer: issuer.address(),
        currency: IssuedCurrencyCode(*b"ABC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"),
        limit: IssuedAmount::from_units(500),
        fee: Amount::from_base_units(1),
        nonce: 0,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_trust_line_set_bound(&mut set, &holder, CHAIN, &genesis).unwrap();
    backend.submit_drc_trust_line_set(set).unwrap();
    mine_template(&mut backend);
    let mut dispatcher = RpcDispatcher::new(backend);
    let line = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcTrustLine".into(),
        params: json!([
            holder.address().to_hex(),
            issuer.address().to_hex(),
            "ABC",
        ]),
    });
    assert_eq!(line.result.as_ref().unwrap()["status"], json!("live"));
}
