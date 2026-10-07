//! Two-node P2P integration for DRC check gossip, full-block transport, and validation.

use std::time::Duration;

use agora_crypto::{
    sign_drc_check_cancel_bound, sign_drc_check_cash_bound, sign_drc_check_create_bound,
    sign_drc_multisign_participant_bound, sign_drc_signer_list_bound, KeyPair,
};
use agora_p2p::{
    dial_addr, fingerprint_topic_tag, trident_network_fingerprint, NetworkConfig, NetworkEvent,
    NetworkMessage, NetworkNode, COMPACT_LANE_DRC_MULTISIGN,
};
use agora_state_machine::{
    apply_block_batched_with_auth_at_blue_score, apply_drc_signer_list, credit_account_into,
    put_burned_supply_into, put_issued_supply_into, put_schema_version_into, AccountJournal,
    StateStore, TxAuthContext, WriteBatch, SCHEMA_VERSION,
};
use agora_types::{
    materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane, Amount, Block,
    BlockHeader, DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, DrcMultisignAuth,
    DrcMultisignEntry, DrcSignerListEntry, DrcSignerListTx, Hash, NativeAssetId, Transaction,
    TxOut, DRC_CHECK_CANCEL_TX_VERSION, DRC_CHECK_CASH_TX_VERSION, DRC_CHECK_CREATE_TX_VERSION,
    DRC_MULTISIGN_AUTH_VERSION,
};
use tokio::time::timeout;

const GENESIS: Hash = Hash([9; 32]);
const CHAIN: &str = "agora-dev";

fn check_create_tx(
    owner: &KeyPair,
    destination: agora_types::Address,
    nonce: u64,
) -> DrcCheckCreateTx {
    let mut tx = DrcCheckCreateTx {
        version: DRC_CHECK_CREATE_TX_VERSION,
        owner: owner.address(),
        destination,
        amount: Amount::from_base_units(10),
        fee: Amount::from_base_units(1),
        destination_tag: None,
        source_tag: None,
        invoice_id: Hash::ZERO,
        expires_after_blue_score: Some(50),
        nonce,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_check_create_bound(&mut tx, owner, CHAIN, &GENESIS).unwrap();
    tx
}

async fn connect_two_nodes(
    fp_a: &str,
    fp_b: &str,
) -> (
    agora_p2p::NetworkHandle,
    tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>,
    agora_p2p::NetworkHandle,
    tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>,
) {
    let (handle_a, mut events_a, node_a) = NetworkNode::build(
        &NetworkConfig::default()
            .with_listen("/ip4/127.0.0.1/tcp/0")
            .with_fingerprint(fp_a),
    )
    .expect("node a");
    let (handle_b, mut events_b, node_b) = NetworkNode::build(
        &NetworkConfig::default()
            .with_listen("/ip4/127.0.0.1/tcp/0")
            .with_fingerprint(fp_b),
    )
    .expect("node b");
    tokio::spawn(node_a.run());
    tokio::spawn(node_b.run());
    let addr_a = wait_listening(&mut events_a).await;
    let dial = dial_addr(&addr_a, handle_a.peer_id());
    handle_b.dial(&dial.to_string()).expect("dial");
    wait_connected(&mut events_b).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    (handle_a, events_a, handle_b, events_b)
}

#[tokio::test]
async fn gossip_check_create_cash_cancel_roundtrip() {
    let _ = tracing_subscriber::fmt::try_init();
    let fp = fingerprint_topic_tag(&trident_network_fingerprint(
        CHAIN,
        &GENESIS,
        &Hash::hash_borsh(&1u64),
    ));
    let (handle_a, mut events_a, handle_b, _) = connect_two_nodes(&fp, &fp).await;

    let owner = KeyPair::from_secret_bytes(&[5; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[6; 32]).unwrap();
    let create = check_create_tx(&owner, destination.address(), 0);
    let id = create.check_id();

    let mut cash = DrcCheckCashTx {
        version: DRC_CHECK_CASH_TX_VERSION,
        submitter: destination.address(),
        check_id: id,
        fee: Amount::from_base_units(1),
        nonce: 0,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_check_cash_bound(&mut cash, &destination, CHAIN, &GENESIS).unwrap();

    let mut cancel = DrcCheckCancelTx {
        version: DRC_CHECK_CANCEL_TX_VERSION,
        submitter: owner.address(),
        check_id: id,
        fee: Amount::from_base_units(1),
        nonce: 2,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    sign_drc_check_cancel_bound(&mut cancel, &owner, CHAIN, &GENESIS).unwrap();

    for msg in [
        NetworkMessage::DrcCheckCreate(create),
        NetworkMessage::DrcCheckCash(cash),
        NetworkMessage::DrcCheckCancel(cancel),
    ] {
        handle_b.publish_message(msg.clone()).expect("publish");
        let _received = timeout(Duration::from_secs(10), async {
            loop {
                match events_a.recv().await {
                    Some(NetworkEvent::Message { message, .. }) if message == msg => break message,
                    Some(_) => continue,
                    None => panic!("closed"),
                }
            }
        })
        .await
        .expect("timeout");
    }

    handle_a.shutdown();
    handle_b.shutdown();
}

#[tokio::test]
async fn mismatched_fingerprint_isolates_check_gossip() {
    let _ = tracing_subscriber::fmt::try_init();
    let fp_a = fingerprint_topic_tag(&trident_network_fingerprint(
        CHAIN,
        &GENESIS,
        &Hash::hash_borsh(&1u64),
    ));
    let fp_b = fingerprint_topic_tag(&trident_network_fingerprint(
        CHAIN,
        &GENESIS,
        &Hash::hash_borsh(&2u64),
    ));
    let (handle_a, mut events_a, handle_b, _) = connect_two_nodes(&fp_a, &fp_b).await;
    let owner = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
    let create = check_create_tx(
        &owner,
        KeyPair::from_secret_bytes(&[8; 32]).unwrap().address(),
        0,
    );
    handle_b
        .publish_message(NetworkMessage::DrcCheckCreate(create))
        .expect("publish");
    let err = timeout(Duration::from_secs(2), async {
        loop {
            if let Some(NetworkEvent::Message {
                message: NetworkMessage::DrcCheckCreate(_),
                ..
            }) = events_a.recv().await
            {
                panic!("incompatible fingerprint delivered check gossip");
            }
        }
    })
    .await;
    assert!(err.is_err(), "expected fingerprint isolation");
    handle_a.shutdown();
    handle_b.shutdown();
}

fn install_signer_list(
    store: &StateStore,
    master: &KeyPair,
    signer: &KeyPair,
    ctx: &TxAuthContext,
) {
    let mut install = DrcSignerListTx::unsigned_set(
        master.address(),
        agora_types::canonical_sorted_entries(&[DrcSignerListEntry {
            signer: signer.address(),
            weight: 1,
        }]),
        1,
        Amount::ZERO,
        0,
    );
    sign_drc_signer_list_bound(&mut install, master, &ctx.chain_id, &ctx.genesis).unwrap();
    let mut batch = WriteBatch::new();
    let mut journal = AccountJournal::default();
    apply_drc_signer_list(store, &install, ctx, &mut batch, &mut journal).unwrap();
    store.write_batch(batch).unwrap();
}

#[tokio::test]
async fn attachment_check_block_uses_full_block_getblock_and_apply() {
    let _ = tracing_subscriber::fmt::try_init();
    let fp = fingerprint_topic_tag(&trident_network_fingerprint(
        CHAIN,
        &GENESIS,
        &Hash::hash_borsh(&1u64),
    ));
    let (handle_a, mut events_a, handle_b, _events_b) = connect_two_nodes(&fp, &fp).await;

    let owner = KeyPair::from_secret_bytes(&[11; 32]).unwrap();
    let destination = KeyPair::from_secret_bytes(&[14; 32]).unwrap();
    let s1 = KeyPair::from_secret_bytes(&[13; 32]).unwrap();
    let store = StateStore::open_in_memory();
    let ctx = TxAuthContext {
        chain_id: CHAIN.into(),
        genesis: GENESIS,
        data_availability_network_fingerprint: None,
    };
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &owner.address(),
        Amount::from_base_units(10_000),
    )
    .unwrap();
    put_issued_supply_into(&mut funding, NativeAssetId::DRC, 10_000);
    for asset in NativeAssetId::ALL {
        put_burned_supply_into(&mut funding, asset, 0);
    }
    put_schema_version_into(&mut funding, SCHEMA_VERSION);
    store.write_batch(funding).unwrap();
    install_signer_list(&store, &owner, &s1, &ctx);

    let mut create = check_create_tx(&owner, destination.address(), 1);
    create.public_key.clear();
    create.signature.clear();
    let signing = create.signing_bytes_bound(&ctx.chain_id, &ctx.genesis);
    let (signer, pk, sig) = sign_drc_multisign_participant_bound(
        owner.address(),
        &signing,
        &s1,
        &ctx.chain_id,
        &ctx.genesis,
    )
    .unwrap();
    create.multisign = Some(DrcMultisignAuth {
        version: DRC_MULTISIGN_AUTH_VERSION,
        signing_for: owner.address(),
        signatures: vec![DrcMultisignEntry {
            signer,
            public_key: pk,
            signature: sig,
        }],
    });

    let mut block = Block::utxo(
        BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 99,
            bits: 0,
            nonce: 0,
            tx_root: Hash::ZERO,
        },
        vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(50),
                address: owner.address(),
            }],
            1,
        )],
    );
    block.drc_check_creates.push(create);
    materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
    block.header.tx_root = block.compute_body_root();
    let hash = block.id();

    match NetworkMessage::compact_from_block(&block) {
        NetworkMessage::TypedCompactBlock(body) => {
            assert!(body
                .lanes
                .iter()
                .any(|lane| lane.kind == COMPACT_LANE_DRC_MULTISIGN));
        }
        other => panic!("expected typed compact, got {other:?}"),
    }

    handle_b
        .publish_message(NetworkMessage::Block(block.clone()))
        .expect("publish full block");

    let received = timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::Message {
                    message: NetworkMessage::Block(b),
                    ..
                }) if b.id() == hash => break b,
                Some(_) => continue,
                None => panic!("closed"),
            }
        }
    })
    .await
    .expect("full block timeout");

    validate_drc_multisign_attachment_lane(&received, &ctx.chain_id, &ctx.genesis).unwrap();
    let result =
        apply_block_batched_with_auth_at_blue_score(&store, &received, 50, Some(&ctx), 1).unwrap();
    store.write_batch(result.batch).unwrap();

    let mut tampered = received;
    tampered.drc_multisign_attachments.clear();
    assert!(
        validate_drc_multisign_attachment_lane(&tampered, &ctx.chain_id, &ctx.genesis).is_err()
    );

    handle_a.shutdown();
    handle_b.shutdown();
}

async fn wait_listening(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>,
) -> libp2p::Multiaddr {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::Listening(addr)) => break addr,
                Some(_) => continue,
                None => panic!("closed"),
            }
        }
    })
    .await
    .expect("listen timeout")
}

async fn wait_connected(events: &mut tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>) {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::PeerConnected(_)) => break,
                Some(_) => continue,
                None => panic!("closed"),
            }
        }
    })
    .await
    .expect("connect timeout");
}
