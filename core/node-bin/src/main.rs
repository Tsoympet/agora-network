//! Agora full node process.
//!
//! Wires consensus, state, p2p, and HTTP JSON-RPC.

mod backend;
mod http;

use std::sync::Arc;

use agora_consensus::{EmissionSchedule, Ghostdag, GhostdagConfig};
use agora_p2p::{NetworkConfig, NetworkEvent, NetworkNode};
use agora_rpc::RpcDispatcher;
use agora_state_machine::{GenesisBuilder, StateStore};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::backend::NodeBackend;
use crate::http::serve_rpc;

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
    let store = Arc::new(StateStore::open(&data_dir).expect("open state store"));
    let genesis_hash = GenesisBuilder::default()
        .ignite(store.as_ref())
        .expect("genesis ignition");

    let (handle, mut events, node) = NetworkNode::build(&net_cfg).expect("p2p build");
    for peer in &bootstrap {
        if let Err(err) = handle.dial(peer) {
            warn!(error = %err, peer, "bootstrap dial failed");
        }
    }

    let rpc_bind =
        std::env::var("AGORA_RPC_BIND").unwrap_or_else(|_| "127.0.0.1:8545".into());
    let allow_fund = matches!(
        std::env::var("AGORA_RPC_ALLOW_FUND").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    );
    let backend = NodeBackend::new(store, Some(handle.clone()), allow_fund);
    let dispatcher = Arc::new(Mutex::new(RpcDispatcher::new(backend)));
    tokio::spawn(serve_rpc(rpc_bind.clone(), dispatcher));

    info!(
        k = ghostdag.config().k,
        initial_reward = emission.initial_reward,
        peer_id = %handle.peer_id(),
        genesis = %genesis_hash.to_hex(),
        %rpc_bind,
        allow_fund,
        "agora-node foundation boot ok"
    );
    println!(
        "Agora Network node — peer {} genesis {} rpc http://{}",
        handle.peer_id(),
        genesis_hash.to_hex(),
        rpc_bind
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
