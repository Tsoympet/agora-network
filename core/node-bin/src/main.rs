//! Agora full node process.
//!
//! Wires consensus, state, p2p, HTTP JSON-RPC, and PoW-gated block admission.

mod admit;
mod backend;
mod http;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agora_consensus::{EmissionSchedule, PowAlgorithm};
use agora_p2p::{
    reconstruct_compact_block, Mempool, NetworkConfig, NetworkEvent, NetworkHandle, NetworkMessage,
    NetworkNode, PendingFetches, ReconstructError,
};
use agora_rpc::RpcDispatcher;
use agora_state_machine::{GenesisBuilder, StateStore};
use agora_types::{Block, Hash};
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

fn admit_gossip_block(chain: &Arc<Mutex<ChainState>>, block: Block) -> Result<Hash, String> {
    chain
        .lock()
        .map_err(|_| "chain lock poisoned".to_string())
        .and_then(|mut guard| guard.admit_block(block).map_err(|e| e.to_string()))
}

fn request_block_if_missing(
    chain: &Arc<Mutex<ChainState>>,
    pending: &mut PendingFetches,
    net: &NetworkHandle,
    hash: Hash,
) {
    let have = chain
        .lock()
        .ok()
        .and_then(|g| g.has_block(&hash).ok())
        .unwrap_or(false);
    if have {
        pending.complete(&hash);
        return;
    }
    if pending.request(hash) {
        if let Err(err) = net.publish_message(NetworkMessage::GetBlock { hash }) {
            warn!(error = %err, hash = %hash.to_hex(), "getblock publish failed");
            pending.complete(&hash);
        } else {
            info!(hash = %hash.to_hex(), "ibd getblock requested");
        }
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
    let mempool = Arc::new(Mutex::new(Mempool::new(10_000)));

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
        mempool.clone(),
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
        "Agora Network node — peer {} genesis {} rpc http://{} pow={:?} bits={}",
        handle.peer_id(),
        genesis_hash.to_hex(),
        rpc_bind,
        pow_algo,
        template_bits
    );

    let net = handle.clone();
    tokio::spawn(async move {
        let mut pending = PendingFetches::new(Duration::from_secs(30));
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
                        let id = block.id();
                        pending.complete(&id);
                        match admit_gossip_block(&chain, block) {
                            Ok(id) => {
                                info!(%peer, %topic, block = %id.to_hex(), "admitted gossip block")
                            }
                            Err(err) => {
                                warn!(%peer, %topic, error = %err, "rejected gossip block")
                            }
                        }
                    }
                    NetworkMessage::CompactBlock { header, short_ids } => {
                        let hash = header.hash();
                        let have = chain
                            .lock()
                            .ok()
                            .and_then(|g| g.has_block(&hash).ok())
                            .unwrap_or(false);
                        if have {
                            pending.complete(&hash);
                            continue;
                        }
                        let lookup = |sid: &[u8; 8]| {
                            mempool
                                .lock()
                                .ok()
                                .and_then(|pool| pool.get_by_short_id(sid).cloned())
                        };
                        match reconstruct_compact_block(header, &short_ids, lookup) {
                            Ok(block) => {
                                pending.complete(&hash);
                                match admit_gossip_block(&chain, block) {
                                    Ok(id) => info!(
                                        %peer,
                                        %topic,
                                        block = %id.to_hex(),
                                        "admitted compact block"
                                    ),
                                    Err(err) => warn!(
                                        %peer,
                                        %topic,
                                        error = %err,
                                        "rejected compact block"
                                    ),
                                }
                            }
                            Err(ReconstructError::MissingShortIds(n)) => {
                                info!(
                                    %peer,
                                    %topic,
                                    missing = n,
                                    hash = %hash.to_hex(),
                                    "compact miss — requesting full block"
                                );
                                request_block_if_missing(&chain, &mut pending, &net, hash);
                            }
                            Err(ReconstructError::TxRootMismatch) => {
                                warn!(
                                    %peer,
                                    %topic,
                                    hash = %hash.to_hex(),
                                    "compact tx_root mismatch — requesting full block"
                                );
                                request_block_if_missing(&chain, &mut pending, &net, hash);
                            }
                        }
                    }
                    NetworkMessage::BlockAnnounce { hash } => {
                        info!(%peer, %topic, announce = %hash.to_hex(), "block announce");
                        request_block_if_missing(&chain, &mut pending, &net, hash);
                    }
                    NetworkMessage::GetBlock { hash } => {
                        let served = chain
                            .lock()
                            .ok()
                            .and_then(|g| g.load_block(&hash).ok())
                            .flatten();
                        match served {
                            Some(block) => {
                                if let Err(err) = net.publish_message(NetworkMessage::Block(block))
                                {
                                    warn!(
                                        %peer,
                                        hash = %hash.to_hex(),
                                        error = %err,
                                        "getblock serve failed"
                                    );
                                } else {
                                    info!(%peer, hash = %hash.to_hex(), "served getblock");
                                }
                            }
                            None => {
                                info!(%peer, hash = %hash.to_hex(), "getblock miss — block unknown locally");
                            }
                        }
                    }
                    NetworkMessage::Transaction(tx) => {
                        match mempool.lock() {
                            Ok(mut pool) => {
                                if let Err(err) = pool.admit(tx) {
                                    warn!(%peer, %topic, error = %err, "tx gossip rejected");
                                } else {
                                    info!(%peer, %topic, "tx gossip admitted");
                                }
                            }
                            Err(_) => warn!(%peer, %topic, "mempool lock poisoned"),
                        }
                    }
                },
            }
        }
    });

    node.run().await;
}
