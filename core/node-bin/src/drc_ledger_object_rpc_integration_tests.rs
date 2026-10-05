//! Node-backed RPC coverage for the canonical DRC object and operation indexes.

use agora_rpc::{RpcBackend, RpcDispatcher, RpcRequest};
use agora_types::{DrcLedgerObjectKey, DrcOperation, Hash};
use serde_json::json;

use super::drc_payment_channel_public_helpers::{funded_fixture, mine_template, signed_create};

#[test]
fn canonical_object_pages_and_receipts_are_public_and_cursor_bound() {
    let mut fixture = funded_fixture();
    let first = signed_create(
        &fixture.owner,
        &fixture.claim_key,
        fixture.destination.address(),
        fixture.genesis,
        0,
        100,
        None,
    );
    let first_operation = DrcOperation::PaymentChannelCreate(first.clone());
    let first_transaction_id = first.channel_id();
    let first_object_id = DrcLedgerObjectKey::PaymentChannel {
        channel_id: first_transaction_id,
    }
    .object_id();
    fixture
        .backend
        .submit_drc_payment_channel_create(first)
        .unwrap();
    let first_block_id = mine_template(&mut fixture.backend);

    let second = signed_create(
        &fixture.owner,
        &fixture.claim_key,
        fixture.destination.address(),
        fixture.genesis,
        1,
        101,
        None,
    );
    let second_object_id = DrcLedgerObjectKey::PaymentChannel {
        channel_id: second.channel_id(),
    }
    .object_id();
    fixture
        .backend
        .submit_drc_payment_channel_create(second)
        .unwrap();
    mine_template(&mut fixture.backend);

    let mut dispatcher = RpcDispatcher::new(fixture.backend);
    let object = dispatcher.handle(RpcRequest {
        id: Some(json!(1)),
        method: "agora_getDrcObject".into(),
        params: json!({ "object_id": first_object_id.to_hex() }),
    });
    assert_eq!(object.result.as_ref().unwrap()["status"], json!("live"));
    assert_eq!(
        object.result.as_ref().unwrap()["object"]["kind"],
        json!("payment_channel")
    );

    let first_page = dispatcher.handle(RpcRequest {
        id: Some(json!(2)),
        method: "agora_getDrcAccountObjects".into(),
        params: json!({
            "account": fixture.owner.address().to_hex(),
            "kind": "payment_channel",
            "limit": 1,
        }),
    });
    let first_page = first_page.result.unwrap();
    assert_eq!(first_page["objects"].as_array().unwrap().len(), 1);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("two live channels require a continuation")
        .to_owned();

    let second_page = dispatcher.handle(RpcRequest {
        id: Some(json!(3)),
        method: "agora_getDrcAccountObjects".into(),
        params: json!({
            "account": fixture.owner.address().to_hex(),
            "kind": "payment_channel",
            "limit": 1,
            "cursor": cursor.clone(),
        }),
    });
    let second_page = second_page.result.unwrap();
    assert_eq!(second_page["objects"].as_array().unwrap().len(), 1);
    assert!(second_page["next_cursor"].is_null());
    let returned_ids = [
        first_page["objects"][0]["object_id"].clone(),
        second_page["objects"][0]["object_id"].clone(),
    ];
    assert!(returned_ids.contains(&serde_json::to_value(first_object_id).unwrap()));
    assert!(returned_ids.contains(&serde_json::to_value(second_object_id).unwrap()));

    let operation = dispatcher.handle(RpcRequest {
        id: Some(json!(4)),
        method: "agora_getDrcOperation".into(),
        params: json!({ "operation_id": first_operation.operation_id().to_hex() }),
    });
    assert_eq!(
        operation.result.as_ref().unwrap()["status"],
        json!("accepted")
    );
    assert_eq!(
        operation.result.as_ref().unwrap()["receipt"]["canonical_block_id"],
        serde_json::to_value(first_block_id).unwrap()
    );

    let transaction = dispatcher.handle(RpcRequest {
        id: Some(json!(5)),
        method: "agora_getDrcTransaction".into(),
        params: json!({ "transaction_id": first_transaction_id.to_hex() }),
    });
    assert_eq!(
        transaction.result.as_ref().unwrap()["status"],
        json!("accepted")
    );
    assert_eq!(
        transaction.result.as_ref().unwrap()["receipt"]["operation_id"],
        serde_json::to_value(first_operation.operation_id()).unwrap()
    );

    let isolated = dispatcher.handle(RpcRequest {
        id: Some(json!(6)),
        method: "agora_getDrcAccountObjects".into(),
        params: json!({
            "account": fixture.destination.address().to_hex(),
            "kind": "payment_channel",
        }),
    });
    assert!(isolated.result.unwrap()["objects"]
        .as_array()
        .unwrap()
        .is_empty());

    let unknown = dispatcher.handle(RpcRequest {
        id: Some(json!(7)),
        method: "agora_getDrcObject".into(),
        params: json!({ "object_id": Hash([0xee; 32]).to_hex() }),
    });
    assert_eq!(unknown.result.unwrap()["status"], json!("unknown"));

    let mut tampered = cursor;
    let last = tampered.pop().unwrap();
    tampered.push(if last == '0' { '1' } else { '0' });
    for params in [
        json!({
            "account": fixture.owner.address().to_hex(),
            "kind": "payment_channel",
            "cursor": tampered,
        }),
        json!({
            "account": fixture.owner.address().to_hex(),
            "cursor": first_page["next_cursor"],
        }),
        json!({
            "account": fixture.owner.address().to_hex(),
            "kind": "payment_channel",
            "cursor": "not-hex",
        }),
    ] {
        let rejected = dispatcher.handle(RpcRequest {
            id: Some(json!(8)),
            method: "agora_getDrcAccountObjects".into(),
            params,
        });
        assert_eq!(rejected.error.unwrap().code, -32602);
    }
}
