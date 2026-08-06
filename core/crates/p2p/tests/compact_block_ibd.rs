//! Two-node compact announce → GetBlock → full Block IBD roundtrip.

use std::time::Duration;

use agora_p2p::{
    dial_addr, reconstruct_compact_block, NetworkConfig, NetworkEvent, NetworkMessage, NetworkNode,
};
use agora_types::{Block, BlockHeader, Hash};
use tokio::time::timeout;

#[tokio::test]
async fn announce_triggers_getblock_and_full_serve() {
    let _ = tracing_subscriber::fmt::try_init();

    let (handle_a, mut events_a, node_a) =
        NetworkNode::build(&NetworkConfig::default().with_listen("/ip4/127.0.0.1/tcp/0"))
            .expect("node a");
    let (handle_b, mut events_b, node_b) =
        NetworkNode::build(&NetworkConfig::default().with_listen("/ip4/127.0.0.1/tcp/0"))
            .expect("node b");

    tokio::spawn(node_a.run());
    tokio::spawn(node_b.run());

    let addr_a = wait_listening(&mut events_a).await;
    let _addr_b = wait_listening(&mut events_b).await;

    let dial = dial_addr(&addr_a, handle_a.peer_id());
    handle_b.dial(&dial.to_string()).expect("dial a");
    wait_connected(&mut events_b).await;
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let header = BlockHeader {
        version: 1,
        parents: vec![Hash::ZERO],
        timestamp_ms: 42,
        bits: 0,
        nonce: 7,
        tx_root: Hash::ZERO,
    };
    let block = Block {
        header: header.clone(),
        transactions: vec![],
        account_transfers: vec![],
        stake_ops: vec![],
    };
    let hash = block.id();

    handle_b
        .publish_message(NetworkMessage::compact_from_block(&block))
        .expect("compact");
    handle_b
        .publish_message(NetworkMessage::BlockAnnounce { hash })
        .expect("announce");

    // A sees compact and/or announce, then requests the body.
    timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::Message {
                    message:
                        NetworkMessage::CompactBlock {
                            header: h,
                            short_ids,
                        },
                    ..
                }) if h.hash() == hash => {
                    let rebuilt = reconstruct_compact_block(h, &short_ids, |_| None).unwrap();
                    assert_eq!(rebuilt.id(), hash);
                    break;
                }
                Some(NetworkEvent::Message {
                    message: NetworkMessage::BlockAnnounce { hash: h },
                    ..
                }) if h == hash => break,
                Some(_) => continue,
                None => panic!("event channel closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for announce/compact");

    handle_a
        .publish_message(NetworkMessage::GetBlock { hash })
        .expect("getblock");

    // B serves when it sees GetBlock.
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
                None => panic!("b closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for getblock on B");

    let received = timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::Message {
                    message: NetworkMessage::Block(b),
                    ..
                }) if b.id() == hash => break b,
                Some(_) => continue,
                None => panic!("a closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for full block on A");

    assert_eq!(received.header.nonce, 7);
    assert!(received.transactions.is_empty());

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
