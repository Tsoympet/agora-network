//! Two-node `/agora/getblock/1` request-response roundtrip.

use std::time::Duration;

use agora_p2p::{dial_addr, NetworkConfig, NetworkEvent, NetworkNode};
use agora_types::{Block, BlockHeader, Hash};
use tokio::time::timeout;

#[tokio::test]
async fn direct_getblock_returns_full_block() {
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
    // Identify / protocol negotiation settle.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let header = BlockHeader {
        version: 1,
        parents: vec![Hash::ZERO],
        timestamp_ms: 99,
        bits: 0,
        nonce: 11,
        tx_root: Hash::ZERO,
    };
    let block = Block {
        header: header.clone(),
        transactions: vec![],
        account_transfers: vec![],
        stake_ops: vec![],
        ovl_executions: vec![],
        drc_payments: vec![],
        data_commitments: vec![],
        drc_account_policies: vec![],
        drc_deposit_preauths: vec![],
        drc_regular_keys: vec![],
        drc_signer_lists: vec![],
        drc_ticket_creates: vec![],
        drc_escrow_creates: vec![],
        drc_escrow_finishes: vec![],
        drc_escrow_cancels: vec![],
        drc_check_creates: vec![],
        drc_check_cashes: vec![],
        drc_check_cancels: vec![],
        drc_payment_channel_creates: vec![],
        drc_payment_channel_funds: vec![],
        drc_payment_channel_claims: vec![],
        drc_payment_channel_closes: vec![],
        drc_trust_line_sets: vec![],
        drc_issued_transfers: vec![],
        drc_issued_asset_policy_sets: vec![],
        drc_trust_line_issuer_controls: vec![],
        drc_issued_clawbacks: vec![],
        drc_offer_creates: vec![],
        drc_offer_cancels: vec![],
        drc_multisign_attachments: vec![],
        tlt_covenants: Vec::new(),
        passport_attestations: Vec::new(),
        hub_registrations: Vec::new(),
        grant_registrations: Vec::new(),
        mission_registrations: Vec::new(),
    };
    let hash = block.id();

    // B asks A for the block; A serves from the test harness.
    handle_b
        .request_block(handle_a.peer_id(), hash)
        .expect("request");

    timeout(Duration::from_secs(10), async {
        loop {
            match events_a.recv().await {
                Some(NetworkEvent::GetBlockRequest {
                    peer,
                    hash: h,
                    request_id,
                }) if h == hash => {
                    assert_eq!(peer, handle_b.peer_id());
                    handle_a
                        .respond_get_block(request_id, Some(block.clone()))
                        .expect("respond");
                    break;
                }
                Some(_) => continue,
                None => panic!("a closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for getblock request on A");

    let received = timeout(Duration::from_secs(10), async {
        loop {
            match events_b.recv().await {
                Some(NetworkEvent::GetBlockResponse {
                    hash: h,
                    block: Some(b),
                    ..
                }) if h == hash => break b,
                Some(NetworkEvent::GetBlockFailure { error, .. }) => {
                    panic!("getblock failed: {error}")
                }
                Some(_) => continue,
                None => panic!("b closed"),
            }
        }
    })
    .await
    .expect("timed out waiting for getblock response on B");

    assert_eq!(received.header.nonce, 11);
    assert_eq!(received.id(), hash);

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
