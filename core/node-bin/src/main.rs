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
    dial_addr, fetch_seeder_peers_best_effort, merge_bootstrap_peers, reconstruct_compact_block,
    Mempool, NetworkConfig, NetworkEvent, NetworkHandle, NetworkMessage, NetworkNode, PeerId,
    PendingFetches, ReconstructError, SeederBook,
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
    peer: PeerId,
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
        // Prefer direct request-response to the announcing peer (no mesh flood).
        if let Err(err) = net.request_block(peer, hash) {
            warn!(
                error = %err,
                %peer,
                hash = %hash.to_hex(),
                "getblock rr failed to enqueue — falling back to gossip"
            );
            if let Err(err) = net.publish_message(NetworkMessage::GetBlock { hash }) {
                warn!(error = %err, hash = %hash.to_hex(), "getblock gossip fallback failed");
                pending.complete(&hash);
            }
        } else {
            info!(%peer, hash = %hash.to_hex(), "ibd getblock rr requested");
        }
    }
}

fn score_peer(net: &NetworkHandle, peer: PeerId, good: bool) {
    let result = if good {
        net.reward_peer(peer)
    } else {
        net.penalize_peer(peer)
    };
    if let Err(err) = result {
        warn!(%peer, error = %err, "peer score update failed");
    }
}

fn gossip_getblock_fallback(pending: &mut PendingFetches, net: &NetworkHandle, hash: Hash) {
    // Allow a single gossip retry after RR failure by clearing the pending slot.
    pending.complete(&hash);
    if !pending.request(hash) {
        return;
    }
    if let Err(err) = net.publish_message(NetworkMessage::GetBlock { hash }) {
        warn!(error = %err, hash = %hash.to_hex(), "getblock gossip fallback failed");
        pending.complete(&hash);
    } else {
        info!(hash = %hash.to_hex(), "ibd getblock gossip fallback");
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
    let dns_seeder = std::env::var("AGORA_DNS_SEEDER")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let seeder_refresh_secs = std::env::var("AGORA_SEEDER_REFRESH_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    let max_peers = std::env::var("AGORA_MAX_PEERS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(64);

    let mut net_cfg = NetworkConfig::default()
        .with_listen(listen)
        .with_bootstrap(bootstrap.clone())
        .with_max_peers(max_peers)
        .with_seeder_refresh_interval(Duration::from_secs(seeder_refresh_secs));
    if let Some(url) = &dns_seeder {
        net_cfg = net_cfg.with_dns_seeder(url.clone());
    }

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

    let mut seeder_book = net_cfg.dns_seeder_url.as_ref().map(|url| {
        SeederBook::new(
            url.clone(),
            bootstrap.clone(),
            net_cfg.max_peers,
            net_cfg.seeder_refresh_interval,
        )
    });
    let seeder_peers = if let Some(url) = net_cfg.dns_seeder_url.clone() {
        fetch_seeder_peers_best_effort(&url).await
    } else {
        Vec::new()
    };
    let dial_peers = merge_bootstrap_peers(&bootstrap, &seeder_peers, net_cfg.max_peers);

    let (handle, mut events, node) = NetworkNode::build(&net_cfg).expect("p2p build");
    for peer in &dial_peers {
        if let Err(err) = handle.dial(peer) {
            warn!(error = %err, peer, "bootstrap dial failed");
        }
    }
    if let Some(book) = seeder_book.as_mut() {
        book.note_dialed(&dial_peers);
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
        daa_bits = chain.lock().map(|c| c.difficulty().as_bits()).unwrap_or(template_bits),
        dns_seeder = net_cfg.dns_seeder_url.as_deref().unwrap_or(""),
        seeder_refresh_secs,
        max_peers = net_cfg.max_peers,
        dial_peers = dial_peers.len(),
        "agora-node foundation boot ok"
    );
    println!(
        "Agora Network node — peer {} genesis {} rpc http://{} pow={:?} bits={}",
        handle.peer_id(),
        genesis_hash.to_hex(),
        rpc_bind,
        pow_algo,
        chain.lock().map(|c| c.difficulty().as_bits()).unwrap_or(template_bits)
    );

    let net = handle.clone();
    let local_peer = handle.peer_id();
    tokio::spawn(async move {
        let mut pending = PendingFetches::new(Duration::from_secs(30));
        let refresh_every = seeder_book
            .as_ref()
            .map(|b| b.refresh_interval())
            .unwrap_or(Duration::ZERO);
        let mut refresh = if refresh_every.is_zero() {
            None
        } else {
            let mut interval = tokio::time::interval(refresh_every);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Skip the immediate first tick; boot already fetched/dialed.
            interval.tick().await;
            Some(interval)
        };

        loop {
            let event = tokio::select! {
                maybe = events.recv() => match maybe {
                    Some(ev) => ev,
                    None => break,
                },
                _ = async {
                    if let Some(interval) = refresh.as_mut() {
                        interval.tick().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    if let Some(book) = seeder_book.as_mut() {
                        let newly = book.refresh_and_dial(|peer| net.dial(peer)).await;
                        if !newly.is_empty() {
                            info!(count = newly.len(), "seeder refresh dialed new peers");
                        }
                    }
                    continue;
                }
            };

            match event {
                NetworkEvent::Listening(addr) => {
                    info!(%addr, "p2p listening");
                    if let Some(book) = seeder_book.as_mut() {
                        let dialable = dial_addr(&addr, local_peer);
                        book.set_dialable(dialable.to_string());
                        match book.register().await {
                            Ok(()) => {
                                info!(%dialable, "registered dialable addr with dns seeder");
                            }
                            Err(err) => {
                                warn!(error = %err, "dns seeder register failed")
                            }
                        }
                    }
                }
                NetworkEvent::PeerConnected(peer) => info!(%peer, "peer connected"),
                NetworkEvent::PeerDisconnected(peer) => info!(%peer, "peer disconnected"),
                NetworkEvent::GetBlockRequest {
                    peer,
                    hash,
                    request_id,
                } => {
                    let block = chain
                        .lock()
                        .ok()
                        .and_then(|g| g.load_block(&hash).ok())
                        .flatten();
                    let found = block.is_some();
                    if let Err(err) = net.respond_get_block(request_id, block) {
                        warn!(
                            %peer,
                            hash = %hash.to_hex(),
                            error = %err,
                            "getblock rr respond failed"
                        );
                    } else if found {
                        info!(%peer, hash = %hash.to_hex(), "served getblock rr");
                    } else {
                        info!(%peer, hash = %hash.to_hex(), "getblock rr miss");
                    }
                }
                NetworkEvent::GetBlockResponse { peer, hash, block } => match block {
                    Some(block) => {
                        pending.complete(&hash);
                        match admit_gossip_block(&chain, block) {
                            Ok(id) => {
                                score_peer(&net, peer, true);
                                info!(%peer, block = %id.to_hex(), "admitted rr getblock")
                            }
                            Err(err) => {
                                score_peer(&net, peer, false);
                                warn!(%peer, error = %err, "rejected rr getblock")
                            }
                        }
                    }
                    None => {
                        warn!(
                            %peer,
                            hash = %hash.to_hex(),
                            "getblock rr remote miss — gossip fallback"
                        );
                        gossip_getblock_fallback(&mut pending, &net, hash);
                    }
                },
                NetworkEvent::GetBlockFailure { peer, hash, error } => {
                    warn!(
                        %peer,
                        hash = %hash.to_hex(),
                        %error,
                        "getblock rr failure — gossip fallback"
                    );
                    score_peer(&net, peer, false);
                    gossip_getblock_fallback(&mut pending, &net, hash);
                }
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
                                score_peer(&net, peer, true);
                                info!(%peer, %topic, block = %id.to_hex(), "admitted gossip block")
                            }
                            Err(err) => {
                                score_peer(&net, peer, false);
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
                                    Ok(id) => {
                                        score_peer(&net, peer, true);
                                        info!(
                                            %peer,
                                            %topic,
                                            block = %id.to_hex(),
                                            "admitted compact block"
                                        );
                                    }
                                    Err(err) => {
                                        score_peer(&net, peer, false);
                                        warn!(
                                            %peer,
                                            %topic,
                                            error = %err,
                                            "rejected compact block"
                                        );
                                    }
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
                                request_block_if_missing(&chain, &mut pending, &net, peer, hash);
                            }
                            Err(ReconstructError::TxRootMismatch) => {
                                warn!(
                                    %peer,
                                    %topic,
                                    hash = %hash.to_hex(),
                                    "compact tx_root mismatch — requesting full block"
                                );
                                request_block_if_missing(&chain, &mut pending, &net, peer, hash);
                            }
                        }
                    }
                    NetworkMessage::BlockAnnounce { hash } => {
                        info!(%peer, %topic, announce = %hash.to_hex(), "block announce");
                        request_block_if_missing(&chain, &mut pending, &net, peer, hash);
                    }
                    NetworkMessage::GetBlock { hash } => {
                        // Legacy / RR-fallback path: still answer over gossip when asked.
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
                                        "getblock gossip serve failed"
                                    );
                                } else {
                                    info!(%peer, hash = %hash.to_hex(), "served getblock gossip");
                                }
                            }
                            None => {
                                info!(
                                    %peer,
                                    hash = %hash.to_hex(),
                                    "getblock gossip miss — block unknown locally"
                                );
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
