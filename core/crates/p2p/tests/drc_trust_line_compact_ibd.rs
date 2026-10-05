//! Trust-line v16 lane: compact guard, GetBlock IBD, attachment tamper isolation.

use std::time::Duration;

use agora_crypto::{sign_drc_trust_line_set_bound, KeyPair};
use agora_p2p::{
    dial_addr, reconstruct_compact_block, NetworkConfig, NetworkEvent, NetworkMessage, NetworkNode,
};
use agora_state_machine::{credit_account_into, StateStore, WriteBatch};
use agora_types::{
    materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane, Address, Amount,
    Block, BlockHeader, DrcMultisignAuth, DrcMultisignEntry, DrcTrustLineSetTx, Hash,
    IssuedAmount, IssuedCurrencyCode, NativeAssetId, DRC_MULTISIGN_AUTH_VERSION,
    DRC_TRUST_LINE_SET_TX_VERSION,
};
use tokio::time::timeout;

const CHAIN: &str = "agora-dev";
const GENESIS: Hash = Hash([9; 32]);

fn trust_line_block_with_attachment() -> Block {
    let store = StateStore::open_in_memory();
    let holder = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
    let issuer = KeyPair::from_secret_bytes(&[8; 32]).unwrap();
    let signer = KeyPair::from_secret_bytes(&[9; 32]).unwrap();
    let mut batch = WriteBatch::new();
    credit_account_into(
        &mut batch,
        &store,
        NativeAssetId::DRC,
        &holder.address(),
        Amount::from_base_units(1000),
    )
    .unwrap();
    store.write_batch(batch).unwrap();
    let currency = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let mut set = DrcTrustLineSetTx {
        version: DRC_TRUST_LINE_SET_TX_VERSION,
        holder: holder.address(),
        issuer: issuer.address(),
        currency,
        limit: IssuedAmount::from_units(100),
        fee: Amount::from_base_units(1),
        nonce: 0,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    let pk = signer.public_key_bytes().to_vec();
    let mut tmp = set.clone();
    sign_drc_trust_line_set_bound(&mut tmp, &holder, CHAIN, &GENESIS).unwrap();
    let sig = tmp.signature.clone();
    set.multisign = Some(DrcMultisignAuth {
        version: DRC_MULTISIGN_AUTH_VERSION,
        signing_for: holder.address(),
        signatures: vec![DrcMultisignEntry {
            signer: signer.address(),
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
        vec![],
    );
    block.drc_trust_line_sets.push(set);
    materialize_drc_multisign_attachments(&mut block, CHAIN, &GENESIS).unwrap();
    block.header.tx_root = block.compute_body_root();
    block
}

#[tokio::test]
async fn trust_line_v16_block_uses_full_body_not_compact() {
    let _ = tracing_subscriber::fmt::try_init();
    let block = trust_line_block_with_attachment();
    assert!(matches!(
        NetworkMessage::compact_from_block(&block),
        NetworkMessage::Block(_)
    ));
}

#[tokio::test]
async fn trust_line_getblock_ibd_roundtrip() {
    let _ = tracing_subscriber::fmt::try_init();
    let block = trust_line_block_with_attachment();
    let hash = block.id();

    let (handle_a, mut events_a, node_a) =
        NetworkNode::build(&NetworkConfig::default().with_listen("/ip4/127.0.0.1/tcp/0"))
            .expect("node a");
    let (handle_b, mut events_b, node_b) =
        NetworkNode::build(&NetworkConfig::default().with_listen("/ip4/127.0.0.1/tcp/0"))
            .expect("node b");
    tokio::spawn(node_a.run());
    tokio::spawn(node_b.run());
    let addr_a = wait_listening(&mut events_a).await;
    handle_b
        .dial(&dial_addr(&addr_a, handle_a.peer_id()).to_string())
        .expect("dial");
    wait_connected(&mut events_b).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    handle_b
        .publish_message(NetworkMessage::BlockAnnounce { hash })
        .expect("announce");
    handle_a
        .publish_message(NetworkMessage::GetBlock { hash })
        .expect("getblock");

    timeout(Duration::from_secs(10), async {
        loop {
            match events_b.recv().await {
                Some(NetworkEvent::Message {
                    message: NetworkMessage::GetBlock { hash: h },
                    ..
                }) if h == hash => {
                    handle_b
                        .publish_message(NetworkMessage::Block(block.clone()))
                        .expect("serve");
                    break;
                }
                Some(_) => continue,
                None => panic!("closed"),
            }
        }
    })
    .await
    .expect("getblock timeout");

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
    assert_eq!(received.drc_trust_line_sets.len(), 1);

    handle_a.shutdown();
    handle_b.shutdown();
}

#[tokio::test]
async fn trust_line_missing_vs_altered_attachment_rejected() {
    let block = trust_line_block_with_attachment();
    validate_drc_multisign_attachment_lane(&block, CHAIN, &GENESIS).unwrap();

    let mut missing = block.clone();
    missing.drc_multisign_attachments.clear();
    assert!(validate_drc_multisign_attachment_lane(&missing, CHAIN, &GENESIS).is_err());

    let mut altered = block.clone();
    altered.drc_multisign_attachments[0].auth.signing_for = Address([0xff; 20]);
    assert!(validate_drc_multisign_attachment_lane(&altered, CHAIN, &GENESIS).is_err());
}

#[test]
fn compact_reconstruct_refuses_trust_line_lane_without_mempool() {
    let block = trust_line_block_with_attachment();
    let header = block.header.clone();
    let sid = agora_p2p::tx_short_id(&block.drc_trust_line_sets[0].trust_line_set_tx_id());
    assert!(reconstruct_compact_block(header, &[sid], |_| None).is_err());
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
