//! Issued-controls v17 lane: compact guard, GetBlock IBD, attachment tamper isolation.

use std::time::Duration;

use agora_crypto::{sign_drc_issued_asset_policy_set_bound, KeyPair};
use agora_p2p::{
    dial_addr, reconstruct_compact_block, NetworkConfig, NetworkEvent, NetworkMessage, NetworkNode,
};
use agora_state_machine::{credit_account_into, StateStore, WriteBatch};
use agora_types::{
    materialize_drc_multisign_attachments, validate_drc_multisign_attachment_lane, Amount, Block,
    BlockHeader, DrcIssuedAssetPolicyAction, DrcIssuedAssetPolicySetTx, DrcMultisignAuth,
    DrcMultisignEntry, Hash, IssuedCurrencyCode, NativeAssetId,
    DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION, DRC_MULTISIGN_AUTH_VERSION,
};
use tokio::time::timeout;

const CHAIN: &str = "agora-dev";
const GENESIS: Hash = Hash([9; 32]);

fn policy_block_with_attachment() -> Block {
    let store = StateStore::open_in_memory();
    let issuer = KeyPair::from_secret_bytes(&[7; 32]).unwrap();
    let signer = KeyPair::from_secret_bytes(&[8; 32]).unwrap();
    let mut batch = WriteBatch::new();
    credit_account_into(
        &mut batch,
        &store,
        NativeAssetId::DRC,
        &issuer.address(),
        Amount::from_base_units(1000),
    )
    .unwrap();
    store.write_batch(batch).unwrap();
    let currency = IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let mut set = DrcIssuedAssetPolicySetTx {
        version: DRC_ISSUED_ASSET_POLICY_SET_TX_VERSION,
        issuer: issuer.address(),
        currency,
        action: DrcIssuedAssetPolicyAction::EnableGlobalFreeze,
        fee: Amount::from_base_units(1),
        nonce: 0,
        account_sequence: None,
        public_key: Vec::new(),
        signature: Vec::new(),
        multisign: None,
    };
    let pk = signer.public_key_bytes().to_vec();
    let mut tmp = set.clone();
    sign_drc_issued_asset_policy_set_bound(&mut tmp, &issuer, CHAIN, &GENESIS).unwrap();
    let sig = tmp.signature.clone();
    set.multisign = Some(DrcMultisignAuth {
        version: DRC_MULTISIGN_AUTH_VERSION,
        signing_for: issuer.address(),
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
    block.drc_issued_asset_policy_sets.push(set);
    materialize_drc_multisign_attachments(&mut block, CHAIN, &GENESIS).unwrap();
    block.header.tx_root = block.compute_body_root();
    block
}

#[tokio::test]
async fn issued_controls_v17_block_uses_typed_compact() {
    let _ = tracing_subscriber::fmt::try_init();
    let block = policy_block_with_attachment();
    match NetworkMessage::compact_from_block(&block) {
        NetworkMessage::TypedCompactBlock(body) => {
            assert!(body
                .lanes
                .iter()
                .any(|lane| lane.kind == agora_p2p::COMPACT_LANE_DRC_MULTISIGN));
        }
        other => panic!("expected typed compact, got {other:?}"),
    }
}

#[tokio::test]
async fn issued_controls_getblock_ibd_roundtrip() {
    let _ = tracing_subscriber::fmt::try_init();
    let block = policy_block_with_attachment();
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
    assert_eq!(received.drc_issued_asset_policy_sets.len(), 1);
    validate_drc_multisign_attachment_lane(&received, CHAIN, &GENESIS).unwrap();

    handle_a.shutdown();
    handle_b.shutdown();
}

#[tokio::test]
async fn issued_controls_missing_vs_altered_attachment_rejected() {
    let block = policy_block_with_attachment();
    validate_drc_multisign_attachment_lane(&block, CHAIN, &GENESIS).unwrap();

    let mut missing = block.clone();
    missing.drc_multisign_attachments.clear();
    assert!(validate_drc_multisign_attachment_lane(&missing, CHAIN, &GENESIS).is_err());

    let mut altered = block.clone();
    altered.drc_multisign_attachments[0].auth.signing_for = agora_types::Address([0xff; 20]);
    assert!(validate_drc_multisign_attachment_lane(&altered, CHAIN, &GENESIS).is_err());
}

#[test]
fn compact_reconstruct_refuses_policy_lane_without_mempool() {
    let block = policy_block_with_attachment();
    let header = block.header.clone();
    let sid = agora_p2p::tx_short_id(&block.drc_issued_asset_policy_sets[0].policy_set_tx_id());
    assert!(reconstruct_compact_block(header, &[sid], |_| None).is_err());
}

#[test]
fn issued_controls_gossip_wire_policy_set_roundtrip() {
    let block = policy_block_with_attachment();
    let msg =
        NetworkMessage::DrcIssuedAssetPolicySet(block.drc_issued_asset_policy_sets[0].clone());
    let encoded = borsh::to_vec(&msg).unwrap();
    let decoded: NetworkMessage = borsh::from_slice(&encoded).unwrap();
    match decoded {
        NetworkMessage::DrcIssuedAssetPolicySet(tx) => {
            tx.validate_structure().unwrap();
        }
        other => panic!("unexpected message {other:?}"),
    }
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
    .expect("connect timeout")
}
