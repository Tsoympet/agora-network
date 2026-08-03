//! Agora full node process.
//!
//! Wires consensus, state, p2p, HTTP JSON-RPC, and PoW-gated block admission.

mod admit;
mod backend;
mod genesis_cli;
mod http;
mod storage_policy;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agora_consensus::PowAlgorithm;
use agora_p2p::{
    dial_addr, fetch_seeder_peers_best_effort, load_or_generate_identity, merge_bootstrap_peers,
    reconstruct_compact_block, Mempool, NetworkConfig, NetworkEvent, NetworkHandle, NetworkMessage,
    NetworkNode, PeerId, PendingFetches, ReconstructError, SeederBook,
};
use agora_rpc::RpcDispatcher;
use agora_state_machine::{ChainParams, NetworkId, StateStore};
use agora_types::{Address, Block, Hash};
use tracing::{info, warn};

use crate::admit::ChainState;
use crate::backend::{admit_transaction, NodeBackend};
use crate::http::serve_rpc;
use crate::storage_policy::StoragePolicy;

fn resolve_chain_params() -> ChainParams {
    let network = std::env::var("AGORA_NETWORK")
        .ok()
        .and_then(|s| NetworkId::parse(&s))
        .unwrap_or(NetworkId::Dev);
    let mut params = ChainParams::for_network(network).unwrap_or_else(|err| {
        eprintln!("agora-node: {err}");
        std::process::exit(1);
    });

    // Dev may override premine / timestamp / bits. Testnet ignores overrides (frozen).
    if network == NetworkId::Dev {
        if let Some(addr) = std::env::var("AGORA_PREMINE_ADDRESS")
            .ok()
            .and_then(|s| Address::parse(&s))
        {
            params = params.with_premine_address(addr);
        }
        if let Some(ts) = std::env::var("AGORA_GENESIS_TIMESTAMP_MS")
            .ok()
            .and_then(|s| s.parse().ok())
        {
            params = params.with_timestamp_ms(ts);
        }
        if let Some(bits) = std::env::var("AGORA_GENESIS_BITS")
            .ok()
            .and_then(|s| s.parse().ok())
        {
            params = params.with_bits(bits);
        }
    } else if std::env::var("AGORA_PREMINE_ADDRESS").is_ok() {
        warn!("AGORA_PREMINE_ADDRESS ignored on frozen network {}", network);
    }

    if let Some(path) = std::env::var_os("AGORA_GENESIS_FILE") {
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!(
                "agora-node: failed to read AGORA_GENESIS_FILE {}: {e}",
                path.to_string_lossy()
            );
            std::process::exit(1);
        });
        let artifact = agora_state_machine::GenesisArtifact::from_json(&raw).unwrap_or_else(|e| {
            eprintln!("agora-node: invalid genesis artifact: {e}");
            std::process::exit(1);
        });
        params = artifact.to_params().unwrap_or_else(|e| {
            eprintln!("agora-node: genesis artifact rejected: {e}");
            std::process::exit(1);
        });
    }

    if let Some(want) = std::env::var("AGORA_EXPECTED_GENESIS")
        .ok()
        .and_then(|s| Hash::from_hex(s.trim()))
    {
        params = params.with_expected_genesis(want);
    }

    params
}

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

fn admit_gossip_block(
    chain: &Arc<Mutex<ChainState>>,
    mempool: &Arc<Mutex<Mempool>>,
    block: Block,
) -> Result<Hash, String> {
    let id = chain
        .lock()
        .map_err(|_| "chain lock poisoned".to_string())
        .and_then(|mut guard| guard.admit_block(block.clone()).map_err(|e| e.to_string()))?;
    if let Ok(mut pool) = mempool.lock() {
        pool.evict_for_block(&block);
    }
    Ok(id)
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
    let mut argv = std::env::args().skip(1);
    if argv.next().as_deref() == Some("genesis") {
        genesis_cli::run(argv);
    }

    tracing_subscriber::fmt::init();

    let chain_params = resolve_chain_params();
    let emission = chain_params.emission.clone();
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
        .with_network(chain_params.network.as_str())
        .with_listen(listen)
        .with_bootstrap(bootstrap.clone())
        .with_max_peers(max_peers)
        .with_seeder_refresh_interval(Duration::from_secs(seeder_refresh_secs));
    if let Some(url) = &dns_seeder {
        net_cfg = net_cfg.with_dns_seeder(url.clone());
    }

    let data_dir = std::env::var("AGORA_DATA").unwrap_or_else(|_| "data/agora-node".into());
    let identity_path = std::path::Path::new(&data_dir).join("p2p").join("identity.key");
    let identity = load_or_generate_identity(&identity_path).expect("p2p identity");
    info!(
        path = %identity_path.display(),
        peer = %identity.public().to_peer_id(),
        "p2p identity ready"
    );
    net_cfg = net_cfg.with_identity(identity);

    let store = Arc::new(StateStore::open(&data_dir).expect("open state store"));
    let storage = StoragePolicy::from_env();
    let premine_address = chain_params.supply.premine_address;
    let expected_genesis = chain_params.expected_genesis;
    let genesis_hash = chain_params
        .builder()
        .with_archival(storage.archival)
        .load_or_ignite_checked(store.as_ref(), expected_genesis)
        .unwrap_or_else(|e| {
            eprintln!("agora-node: genesis error: {e}");
            std::process::exit(1);
        });
    info!(
        network = %chain_params.network,
        premine = %premine_address,
        genesis = %genesis_hash.to_hex(),
        "genesis ready"
    );

    let chain = Arc::new(Mutex::new(
        ChainState::bootstrap(
            store.clone(),
            genesis_hash,
            pow_algo,
            template_bits,
            storage,
        )
        .expect("chain bootstrap"),
    ));
    info!(
        %data_dir,
        archival = storage.archival,
        hot_window = storage.hot_window,
        "state store opened"
    );
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
    let miner_address = std::env::var("AGORA_MINER_ADDRESS")
        .ok()
        .and_then(|s| Address::parse(&s))
        .unwrap_or(Address::ZERO);
    let connected_peers = Arc::new(AtomicU32::new(0));
    let backend = NodeBackend::new(
        chain.clone(),
        store.clone(),
        Some(handle.clone()),
        allow_fund,
        mempool.clone(),
        miner_address,
        connected_peers.clone(),
        chain_params.network.as_str(),
        genesis_hash,
    );
    let dispatcher = Arc::new(tokio::sync::Mutex::new(RpcDispatcher::new(backend)));
    tokio::spawn(serve_rpc(rpc_bind.clone(), dispatcher.clone()));

    info!(
        network = %chain_params.network,
        initial_reward = emission.initial_reward,
        peer_id = %handle.peer_id(),
        genesis = %genesis_hash.to_hex(),
        %rpc_bind,
        allow_fund,
        miner = %miner_address,
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
        "Agora Network node — network {} peer {} genesis {} rpc http://{} pow={:?} bits={}",
        chain_params.network,
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
                NetworkEvent::PeerConnected(peer) => {
                    let n = connected_peers.fetch_add(1, Ordering::Relaxed) + 1;
                    info!(%peer, connected = n, "peer connected");
                }
                NetworkEvent::PeerDisconnected(peer) => {
                    let prev = connected_peers.load(Ordering::Relaxed);
                    let n = prev.saturating_sub(1);
                    connected_peers.store(n, Ordering::Relaxed);
                    info!(%peer, connected = n, "peer disconnected");
                }
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
                        match admit_gossip_block(&chain, &mempool, block) {
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
                        match admit_gossip_block(&chain, &mempool, block) {
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
                                match admit_gossip_block(&chain, &mempool, block) {
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
                        match admit_transaction(store.as_ref(), &mempool, tx) {
                            Ok(id) => {
                                info!(
                                    %peer,
                                    %topic,
                                    tx = %id.to_hex(),
                                    "tx gossip admitted"
                                );
                            }
                            Err(err) => {
                                warn!(%peer, %topic, error = %err, "tx gossip rejected");
                            }
                        }
                    }
                },
            }
        }
    });

    node.run().await;
}
