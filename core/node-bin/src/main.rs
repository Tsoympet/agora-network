//! Agora full node process.
//!
//! Wires consensus, state, p2p, HTTP JSON-RPC, and PoW-gated block admission.

mod admit;
mod backend;
mod http;

use std::sync::{Arc, Mutex};

use agora_consensus::{EmissionSchedule, PowAlgorithm};
use agora_p2p::{NetworkConfig, NetworkEvent, NetworkMessage, NetworkNode};
use agora_rpc::RpcDispatcher;
use agora_state_machine::{GenesisBuilder, StateStore};
use tracing::{info, warn};

use crate::admit::ChainState;
use crate::backend::NodeBackend;
use crate::http::serve_rpc;

fn parse_pow_algo() -> PowAlgorithm {
    match std::env::var("AGORA_POW_ALGO")
        .unwrap_or_else(|_| "randomx".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "kheavyhash" | "kheavy" | "asic" => PowAlgorithm::KHeavyHash,
        _ => PowAlgorithm::RandomX,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let emission = EmissionSchedule::default();
    let pow_algo = parse_pow_algo();
    let template_bits = std::env::var("AGORA_TEMPLATE_BITS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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

    let chain = Arc::new(Mutex::new(
        ChainState::bootstrap(store.clone(), genesis_hash, pow_algo, template_bits)
            .expect("chain bootstrap"),
    ));

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
    let backend = NodeBackend::new(
        chain.clone(),
        store,
        Some(handle.clone()),
        allow_fund,
    );
    let dispatcher = Arc::new(tokio::sync::Mutex::new(RpcDispatcher::new(backend)));
    tokio::spawn(serve_rpc(rpc_bind.clone(), dispatcher.clone()));

    info!(
        initial_reward = emission.initial_reward,
        peer_id = %handle.peer_id(),
        genesis = %genesis_hash.to_hex(),
        %rpc_bind,
        allow_fund,
        ?pow_algo,
        template_bits,
        "agora-node foundation boot ok"
    );
    println!(
        "Agora Network node — peer {} genesis {} rpc http://{} pow={:?}",
        handle.peer_id(),
        genesis_hash.to_hex(),
        rpc_bind,
        pow_algo
    );

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                NetworkEvent::Listening(addr) => info!(%addr, "p2p listening"),
                NetworkEvent::PeerConnected(peer) => info!(%peer, "peer connected"),
                NetworkEvent::PeerDisconnected(peer) => info!(%peer, "peer disconnected"),
                NetworkEvent::Message {
                    peer,
                    topic,
                    message,
                } => match message {
                    NetworkMessage::Block(block) => {
                        let result = chain
                            .lock()
                            .map_err(|_| "chain lock poisoned".to_string())
                            .and_then(|mut guard| {
                                guard.admit_block(block).map_err(|e| e.to_string())
                            });
                        match result {
                            Ok(id) => {
                                info!(%peer, %topic, block = %id.to_hex(), "admitted gossip block")
                            }
                            Err(err) => {
                                warn!(%peer, %topic, error = %err, "rejected gossip block")
                            }
                        }
                    }
                    NetworkMessage::BlockAnnounce { hash } => {
                        info!(%peer, %topic, announce = %hash.to_hex(), "block announce")
                    }
                    NetworkMessage::Transaction(_) => {
                        info!(%peer, %topic, "tx gossip")
                    }
                },
            }
        }
    });

    node.run().await;
}
