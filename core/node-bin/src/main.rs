//! Agora full node process.
//!
//! Wires consensus, state, p2p, and RPC once each crate reaches its roadmap phase.

use agora_consensus::{EmissionSchedule, Ghostdag, GhostdagConfig};
use agora_p2p::{NetworkConfig, NetworkEvent, NetworkNode};
use agora_state_machine::{GenesisBuilder, StateStore};
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let ghostdag = Ghostdag::new(GhostdagConfig::default());
    let emission = EmissionSchedule::default();

    let listen = std::env::var("AGORA_LISTEN")
        .unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/16111".into());
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

    let net_cfg = NetworkConfig::default()
        .with_listen(listen)
        .with_bootstrap(bootstrap.clone());

    let data_dir = std::env::var("AGORA_DATA").unwrap_or_else(|_| "data/agora-node".into());
    let store = StateStore::open(&data_dir).expect("open state store");
    let genesis_hash = GenesisBuilder::default()
        .ignite(&store)
        .expect("genesis ignition");

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
        "agora-node foundation boot ok"
    );
    println!(
        "Agora Network node — peer {} genesis {}",
        handle.peer_id(),
        genesis_hash.to_hex()
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
