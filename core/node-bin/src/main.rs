//! Agora full node process.
//!
//! Boots genesis through the transaction acceptance layer, binds the datadir to
//! the network fingerprint, and gossips on fingerprint-scoped topics.

use agora_consensus::{EmissionSchedule, Ghostdag, GhostdagConfig};
use agora_p2p::{NetworkConfig, NetworkEvent, NetworkNode};
use agora_state_machine::{
    assert_datadir_fingerprint, load_network_fingerprint, meta_keys, ColumnFamily, GenesisBuilder,
    StateStore,
};
use agora_types::Hash;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let ghostdag = Ghostdag::new(GhostdagConfig::default());
    let emission = EmissionSchedule::default();

    let listen = std::env::var("AGORA_LISTEN").unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/16111".into());
    let bootstrap = std::env::var("AGORA_BOOTSTRAP")
        .ok()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let data_dir = std::env::var("AGORA_DATA").unwrap_or_else(|_| "data/agora-node".into());
    let store = StateStore::open(&data_dir).expect("open state store");

    let (genesis_hash, fingerprint) = match load_network_fingerprint(&store).expect("load fp") {
        Some(fp) => {
            assert_datadir_fingerprint(&store, &fp).expect("datadir fingerprint");
            let genesis_bytes = store
                .get_cf(ColumnFamily::Meta, meta_keys::GENESIS_HASH)
                .expect("read genesis")
                .expect("genesis hash missing from bound datadir");
            if genesis_bytes.len() != 32 {
                panic!(
                    "corrupt genesis hash in datadir (expected 32 bytes, got {})",
                    genesis_bytes.len()
                );
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&genesis_bytes);
            (Hash(bytes), fp)
        }
        None => GenesisBuilder::default()
            .ignite(&store)
            .expect("genesis ignition"),
    };

    let net_cfg = NetworkConfig::default()
        .with_listen(listen)
        .with_bootstrap(bootstrap.clone())
        .with_fingerprint(fingerprint.clone());

    let (handle, mut events, node) = NetworkNode::build(&net_cfg).expect("p2p build");
    for peer in &bootstrap {
        if let Err(err) = handle.dial(peer) {
            warn!(error = %err, peer, "bootstrap dial failed");
        }
    }

    info!(
        k = ghostdag.config().k,
        initial_reward = emission.initial_reward,
        peer_id = %handle.peer_id(),
        genesis = %genesis_hash.to_hex(),
        fingerprint = %fingerprint.digest_hex(),
        "agora-node foundation boot ok"
    );
    println!(
        "Agora Network node — peer {} genesis {} fingerprint {}",
        handle.peer_id(),
        genesis_hash.to_hex(),
        fingerprint.digest_hex()
    );

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                NetworkEvent::Listening(addr) => info!(%addr, "p2p listening"),
                NetworkEvent::PeerConnected(peer) => info!(%peer, "peer connected"),
                NetworkEvent::PeerDisconnected(peer) => info!(%peer, "peer disconnected"),
                NetworkEvent::Message { peer, topic, .. } => {
                    info!(%peer, %topic, "gossip message")
                }
            }
        }
    });

    node.run().await;
}
