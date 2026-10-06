//! Two-node P2P integration for DRC trust line gossip and full-block apply.

use std::time::Duration;

use agora_crypto::{sign_drc_trust_line_set_bound, KeyPair};
use agora_p2p::{
    dial_addr, fingerprint_topic_tag, trident_network_fingerprint, NetworkConfig, NetworkEvent,
    NetworkMessage, NetworkNode,
};
use agora_state_machine::{
    apply_block_batched_with_auth_at_blue_score, credit_account_into, put_burned_supply_into,
    put_issued_supply_into, put_schema_version_into, StateStore, TxAuthContext, WriteBatch,
    SCHEMA_VERSION,
};
use agora_types::{
    materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane, Amount, Block,
    BlockHeader, DrcTrustLineSetTx, Hash, IssuedAmount, IssuedCurrencyCode, NativeAssetId,
    Transaction, TxOut, DRC_TRUST_LINE_SET_TX_VERSION,
};
use tokio::time::timeout;

const CHAIN: &str = "agora-dev";
const GENESIS: Hash = Hash([9; 32]);

fn trust_line_set_tx(holder: &KeyPair, issuer: &KeyPair) -> DrcTrustLineSetTx {
    let currency = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let mut tx = DrcTrustLineSetTx {
        version: DRC_TRUST_LINE_SET_TX_VERSION,
        holder: holder.address(),
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
    sign_drc_trust_line_set_bound(&mut tx, holder, CHAIN, &GENESIS).unwrap();
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
    handle_b
        .dial(&dial_addr(&addr_a, handle_a.peer_id()).to_string())
        .expect("dial");
    wait_connected(&mut events_b).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    (handle_a, events_a, handle_b, events_b)
}

#[tokio::test]
async fn gossip_trust_line_set_roundtrip() {
    let _ = tracing_subscriber::fmt::try_init();
    let fp = fingerprint_topic_tag(&trident_network_fingerprint(
        CHAIN,
        &GENESIS,
        &Hash::hash_borsh(&1u64),
    ));
    let (handle_a, mut events_a, handle_b, _) = connect_two_nodes(&fp, &fp).await;

    let holder = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[8; 32]).unwrap();
    let set = trust_line_set_tx(&holder, &issuer);

    handle_b
        .publish_message(NetworkMessage::DrcTrustLineSet(set.clone()))
        .expect("publish set");

    timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::Message {
                    message: NetworkMessage::DrcTrustLineSet(received),
                    ..
                }) if received.trust_line_set_tx_id() == set.trust_line_set_tx_id() => break,
                Some(_) => continue,
                None => panic!("closed"),
            }
        }
    })
    .await
    .expect("set gossip timeout");

    handle_a.shutdown();
    handle_b.shutdown();
}

#[tokio::test]
async fn fingerprint_mismatch_isolates_trust_line_gossip() {
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

    let holder = KeyPair::from_secret_bytes(&[9; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[10; 32]).unwrap();
    let set = trust_line_set_tx(&holder, &issuer);
    handle_b
        .publish_message(NetworkMessage::DrcTrustLineSet(set))
        .expect("publish");

    let received = timeout(Duration::from_secs(2), events_a.recv()).await;
    assert!(
        received.is_err()
            || !matches!(
                received.ok().flatten(),
                Some(NetworkEvent::Message {
                    message: NetworkMessage::DrcTrustLineSet(_),
                    ..
                })
            )
    );

    handle_a.shutdown();
    handle_b.shutdown();
}

#[tokio::test]
async fn full_block_trust_line_attachment_apply_and_tamper_rejected() {
    let _ = tracing_subscriber::fmt::try_init();
    let fp = fingerprint_topic_tag(&trident_network_fingerprint(
        CHAIN,
        &GENESIS,
        &Hash::hash_borsh(&1u64),
    ));
    let (handle_a, mut events_a, handle_b, _) = connect_two_nodes(&fp, &fp).await;

    let store = StateStore::open_in_memory();
    let holder = KeyPair::from_secret_bytes(&[11; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[12; 32]).unwrap();
    let mut funding = WriteBatch::new();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &holder.address(),
        Amount::from_base_units(10_000),
    )
    .unwrap();
    credit_account_into(
        &mut funding,
        &store,
        NativeAssetId::DRC,
        &issuer.address(),
        Amount::from_base_units(10_000),
    )
    .unwrap();
    put_issued_supply_into(&mut funding, NativeAssetId::DRC, 20_000);
    for asset in NativeAssetId::ALL {
        put_burned_supply_into(&mut funding, asset, 0);
    }
    put_schema_version_into(&mut funding, SCHEMA_VERSION);
    store.write_batch(funding).unwrap();
    let ctx = TxAuthContext {
        chain_id: CHAIN.into(),
        genesis: GENESIS,
        data_availability_network_fingerprint: None,
    };

    let set = trust_line_set_tx(&holder, &issuer);
    let mut block = Block::utxo(
        BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 1,
            bits: 0,
            nonce: 0,
            tx_root: Hash::ZERO,
        },
        vec![Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(50),
                address: holder.address(),
            }],
            1,
        )],
    );
    block.drc_trust_line_sets.push(set);
    materialize_drc_multisign_attachments(&mut block, &ctx.chain_id, &ctx.genesis).unwrap();
    block.header.tx_root = block.compute_body_root();
    let hash = block.id();

    handle_b
        .publish_message(NetworkMessage::Block(block.clone()))
        .expect("publish block");

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
    .expect("block timeout");

    validate_drc_multisign_attachment_lane(&received, &ctx.chain_id, &ctx.genesis).unwrap();
    let result =
        apply_block_batched_with_auth_at_blue_score(&store, &received, 50, Some(&ctx), 1).unwrap();
    store.write_batch(result.batch).unwrap();

    let mut tampered = received;
    tampered.drc_trust_line_sets[0].signature.clear();
    assert!(
        apply_block_batched_with_auth_at_blue_score(&store, &tampered, 50, Some(&ctx), 2).is_err()
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
