//! Two-node `/agora/getheaders/1` request-response roundtrip.

use std::time::Duration;

use agora_p2p::{
    dial_addr, GetHeadersRequest, NetworkConfig, NetworkEvent, NetworkNode,
    MAX_HEADERS_PER_RESPONSE,
};
use agora_types::{BlockHeader, Hash};
use tokio::time::timeout;

#[tokio::test]
async fn direct_getheaders_returns_spine() {
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
    tokio::time::sleep(Duration::from_millis(500)).await;

    let genesis = Hash::hash_bytes(b"genesis");
    let h1 = BlockHeader {
        version: 1,
        parents: vec![genesis],
        timestamp_ms: 1,
        bits: 0,
        nonce: 1,
        tx_root: Hash::ZERO,
    };
    let h1_id = h1.hash();
    let h2 = BlockHeader {
        version: 1,
        parents: vec![h1_id],
        timestamp_ms: 2,
        bits: 0,
        nonce: 2,
        tx_root: Hash::ZERO,
    };

    handle_b
        .request_headers(
            handle_a.peer_id(),
            GetHeadersRequest::new(vec![genesis], MAX_HEADERS_PER_RESPONSE),
        )
        .expect("request headers");

    timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::GetHeadersRequest {
                    peer,
                    locator,
                    request_id,
                    ..
                }) => {
                    assert_eq!(peer, handle_b.peer_id());
                    assert_eq!(locator, vec![genesis]);
                    handle_a
                        .respond_get_headers(request_id, vec![h1.clone(), h2.clone()])
                        .expect("respond");
                    break;
                }
                Some(_) => continue,
                None => panic!("a closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for getheaders request on A");

    let received = timeout(Duration::from_secs(10), async {
        loop {
            match events_b.recv().await {
                Some(NetworkEvent::GetHeadersResponse { headers, .. }) => break headers,
                Some(NetworkEvent::GetHeadersFailure { error, .. }) => {
                    panic!("getheaders failed: {error}")
                }
                Some(_) => continue,
                None => panic!("b closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for getheaders response on B");

    assert_eq!(received.len(), 2);
    assert_eq!(received[0].nonce, 1);
    assert_eq!(received[1].nonce, 2);

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
