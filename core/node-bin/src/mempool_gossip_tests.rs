//! Two-node gossip: a Trident-bound TLT spend admitted on A must land pending on B
//! without waiting for a mined block.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction_bound, Bip44Path};
use agora_p2p::{dial_addr, Mempool, NetworkConfig, NetworkEvent, NetworkMessage, NetworkNode};
use agora_rpc::{RpcBackend, TxStatus};
use agora_state_machine::{ColumnFamily, GenesisBuilder, StateStore};
use agora_types::{Amount, Block, OutPoint, Transaction, TxIn, TxOut};
use borsh::BorshDeserialize;
use tokio::time::timeout;

use crate::admit::ChainState;
use crate::backend::{admit_transaction, resolve_gossip_tx_auth, NodeBackend, NodeBackendConfig};
use crate::storage_policy::StoragePolicy;

const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const EXPERIMENTAL_CHAIN_ID: &str = "agora-trident-experimental-testnet-1";

fn funded_store() -> (Arc<StateStore>, agora_types::Hash, Transaction) {
    let store = Arc::new(StateStore::open_in_memory());
    let seed = seed_from_mnemonic(PHRASE, "").unwrap();
    let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
    let to = derive_bip44(&seed, &Bip44Path::external(1))
        .unwrap()
        .address();
    let genesis = GenesisBuilder::default()
        .with_premine_address(from.address())
        .ignite(&store)
        .unwrap();
    let genesis_block = {
        let bytes = store
            .get_cf(ColumnFamily::Hot, genesis.as_bytes())
            .unwrap()
            .unwrap();
        Block::try_from_slice(&bytes).unwrap()
    };
    let premine_txid = genesis_block.transactions[0].tx_id();
    let premine = Amount::from_whole(10_000_000).unwrap();
    let pay = Amount::from_whole(1).unwrap().as_base_units();
    let fee = 1u64;
    let mut spend = Transaction::unsigned(
        1,
        vec![TxIn {
            previous_outpoint: OutPoint {
                tx_id: premine_txid,
                index: 0,
            },
        }],
        vec![
            TxOut {
                value: Amount::from_base_units(pay),
                address: to,
            },
            TxOut {
                value: Amount::from_base_units(premine.as_base_units() - pay - fee),
                address: from.address(),
            },
        ],
        7,
    );
    sign_transaction_bound(&mut spend, &from, EXPERIMENTAL_CHAIN_ID, &genesis).unwrap();
    (store, genesis, spend)
}

fn chain_for(store: Arc<StateStore>, genesis: agora_types::Hash) -> Arc<Mutex<ChainState>> {
    Arc::new(Mutex::new(
        ChainState::bootstrap_with(
            store,
            genesis,
            crate::admit::ChainBootConfig {
                chain_id: EXPERIMENTAL_CHAIN_ID.into(),
                ..crate::admit::ChainBootConfig::default()
            },
            StoragePolicy::default(),
        )
        .unwrap(),
    ))
}

fn backend_for(
    chain: Arc<Mutex<ChainState>>,
    store: Arc<StateStore>,
    mempool: Arc<Mutex<Mempool>>,
    genesis: agora_types::Hash,
    net: agora_p2p::NetworkHandle,
) -> NodeBackend {
    NodeBackend::new(
        chain,
        store,
        mempool,
        NodeBackendConfig {
            net: Some(net),
            allow_fund: false,
            miner_address: agora_types::Address::ZERO,
            connected_peers: Arc::new(std::sync::atomic::AtomicU32::new(1)),
            network: "testnet".into(),
            genesis_hash: genesis,
        },
    )
}

async fn wait_dial_target(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>,
    peer_id: agora_p2p::PeerId,
) -> String {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::Listening(addr)) => break dial_addr(&addr, peer_id).to_string(),
                Some(_) => continue,
                None => panic!("event channel closed"),
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
                None => panic!("event channel closed"),
            }
        }
    })
    .await
    .expect("connect timeout");
}

#[tokio::test]
async fn experimental_bound_spend_gossips_into_peer_mempool() {
    let (store_a, genesis, spend) = funded_store();
    let (store_b, genesis_b, _) = funded_store();
    assert_eq!(genesis, genesis_b);

    let chain_a = chain_for(store_a.clone(), genesis);
    let chain_b = chain_for(store_b.clone(), genesis);
    let mempool_a = Arc::new(Mutex::new(Mempool::new(32)));
    let mempool_b = Arc::new(Mutex::new(Mempool::new(32)));

    let fingerprint = format!("exp-{}", &genesis.to_hex()[..16]);
    let cfg = |listen: &str| {
        let mut cfg = NetworkConfig::default().with_listen(listen);
        cfg.network = "testnet".into();
        cfg.fingerprint = fingerprint.clone();
        cfg
    };
    let (handle_a, mut events_a, node_a) =
        NetworkNode::build(&cfg("/ip4/127.0.0.1/tcp/0")).expect("node a");
    let (handle_b, mut events_b, node_b) =
        NetworkNode::build(&cfg("/ip4/127.0.0.1/tcp/0")).expect("node b");
    tokio::spawn(node_a.run());
    tokio::spawn(node_b.run());

    let dial_a = wait_dial_target(&mut events_a, handle_a.peer_id()).await;
    let _ = wait_dial_target(&mut events_b, handle_b.peer_id()).await;
    handle_b.dial(&dial_a).expect("dial a");
    wait_connected(&mut events_b).await;
    tokio::time::sleep(Duration::from_millis(1_000)).await;

    let gossip_auth = resolve_gossip_tx_auth(&chain_b, "testnet", genesis, None);
    assert_eq!(gossip_auth.chain_id, EXPERIMENTAL_CHAIN_ID);

    let mut backend_a = backend_for(chain_a, store_a, mempool_a, genesis, handle_a.clone());
    let backend_b = backend_for(
        chain_b.clone(),
        store_b.clone(),
        mempool_b.clone(),
        genesis,
        handle_b.clone(),
    );
    assert_eq!(
        backend_a.get_node_info().unwrap().chain_id.as_deref(),
        Some(EXPERIMENTAL_CHAIN_ID)
    );

    let tx_id = backend_a.submit_transaction(spend.clone()).unwrap();
    assert_eq!(
        backend_a.get_transaction(&tx_id).unwrap().status,
        TxStatus::Pending
    );

    let received = timeout(Duration::from_secs(10), async {
        loop {
            match events_b.recv().await {
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
    assert_eq!(received.tx_id(), tx_id);

    let admitted = admit_transaction(store_b.as_ref(), &mempool_b, received, &gossip_auth)
        .expect("B gossip admit with live Trident auth");
    assert_eq!(admitted, tx_id);
    assert_eq!(
        backend_b.get_transaction(&tx_id).unwrap().status,
        TxStatus::Pending
    );
    let pool = backend_b.get_mempool(16).unwrap();
    assert!(
        pool.iter().any(|entry| entry.tx_id == tx_id),
        "B agora_getMempool must list the gossiped spend"
    );

    handle_a.shutdown();
    handle_b.shutdown();
}
