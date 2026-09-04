use std::time::Duration;

use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
use agora_p2p::{dial_addr, Mempool, NetworkConfig, NetworkEvent, NetworkMessage, NetworkNode};
use agora_types::{Amount, Hash, OutPoint, Transaction, TxIn, TxOut};
use tokio::time::timeout;

const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[tokio::test]
async fn two_nodes_exchange_signed_transaction() {
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

    // Allow gossipsub mesh to form after the connection.
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let seed = seed_from_mnemonic(PHRASE, "").unwrap();
    let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
    let mut tx = Transaction::unsigned(
        1,
        vec![TxIn {
            previous_outpoint: OutPoint {
                tx_id: Hash::ZERO,
                index: 0,
            },
        }],
        vec![TxOut {
            value: Amount::from_base_units(5),
            address: kp.address(),
        }],
        9,
    );
    sign_transaction(&mut tx, &kp).unwrap();
    let mut pool = Mempool::new(32);
    pool.admit(tx.clone()).unwrap();

    handle_b
        .publish_message(NetworkMessage::Transaction(tx))
        .expect("publish");

    let received = timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::Message {
                    message: NetworkMessage::Transaction(tx),
                    ..
                }) => break tx,
                Some(_) => continue,
                None => panic!("event channel closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for gossip tx");

    assert_eq!(received.nonce, 9);
    let mut remote_pool = Mempool::new(32);
    remote_pool.admit(received).expect("remote admission");

    handle_a.shutdown();
    handle_b.shutdown();
}

#[tokio::test]
async fn duplicate_and_self_dials_do_not_corrupt_disconnect_tracking() {
    let (handle_a, mut events_a, node_a) =
        NetworkNode::build(&NetworkConfig::default().with_listen("/ip4/127.0.0.1/tcp/0"))
            .expect("node a");
    let (handle_b, mut events_b, node_b) =
        NetworkNode::build(&NetworkConfig::default().with_listen("/ip4/127.0.0.1/tcp/0"))
            .expect("node b");

    let task_a = tokio::spawn(node_a.run());
    let task_b = tokio::spawn(node_b.run());

    let addr_a = wait_listening(&mut events_a).await;
    let addr_b = wait_listening(&mut events_b).await;
    handle_b
        .dial(&dial_addr(&addr_a, handle_a.peer_id()).to_string())
        .expect("dial a");
    wait_connected(&mut events_a).await;
    wait_connected(&mut events_b).await;

    handle_a
        .dial(&dial_addr(&addr_b, handle_b.peer_id()).to_string())
        .expect("duplicate dial b");
    handle_a
        .dial(&dial_addr(&addr_a, handle_a.peer_id()).to_string())
        .expect("self dial a");
    tokio::time::sleep(Duration::from_millis(100)).await;

    handle_b.shutdown();
    task_b.await.expect("node b task");
    wait_disconnected(&mut events_a).await;
    assert!(!task_a.is_finished(), "node a panicked on peer disconnect");

    handle_a.shutdown();
    task_a.await.expect("node a task");
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

async fn wait_disconnected(events: &mut tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>) {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::PeerDisconnected(_)) => break,
                Some(_) => continue,
                None => panic!("closed"),
            }
        }
    })
    .await
    .expect("disconnect timeout");
}
