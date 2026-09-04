//! Agora full node process.
//!
//! Wires consensus, state, p2p, HTTP JSON-RPC, and PoW-gated block admission.

mod admit;
mod backend;
mod civic;
mod genesis_cli;
mod http;
mod storage_policy;

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agora_consensus::PowAlgorithm;
use agora_p2p::{
    dial_addr, drain_orphans_after, fetch_seeder_peers_best_effort, load_or_generate_identity,
    merge_bootstrap_peers, reconstruct_compact_block, validate_header_chain, FetchReason,
    GetHeadersRequest, Mempool, NetworkConfig, NetworkEvent, NetworkHandle, NetworkMessage,
    NetworkNode, OrphanPool, PeerId, PendingFetches, ReconstructError, SeederBook,
    MAX_HEADERS_PER_RESPONSE,
};
use agora_rpc::RpcDispatcher;
use agora_state_machine::{
    delete_orphan, list_orphans, store_orphan, ChainParams, NetworkId, StateStore,
};
use agora_types::{Address, Block, Hash};
use tracing::{info, warn};

use crate::admit::{AdmitError, ChainBootConfig, ChainState};
use crate::backend::{admit_transaction, NodeBackend};
use crate::http::{enforce_rpc_bind_policy, serve_rpc, RpcHttpConfig};
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
        warn!(
            "AGORA_PREMINE_ADDRESS ignored on frozen network {}",
            network
        );
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

fn parse_pow_algo_env() -> Option<PowAlgorithm> {
    std::env::var("AGORA_POW_ALGO")
        .ok()
        .map(|s| match s.to_ascii_lowercase().as_str() {
            "kheavyhash" | "kheavy" | "asic" => PowAlgorithm::KHeavyHash,
            _ => PowAlgorithm::RandomX,
        })
}

/// Resolve PoW + initial bits from [`ChainParams`], allowing env overrides on `dev` only.
fn resolve_boot_config(params: &ChainParams) -> ChainBootConfig {
    let mut boot = ChainBootConfig::from(params);

    if params.network == NetworkId::Dev {
        // Dev keeps a separate template/DAA initial level (default 1) from genesis bits.
        let bits = std::env::var("AGORA_TEMPLATE_BITS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        boot.initial_bits = bits;
        boot.daa.min_level = if bits == 0 {
            0
        } else {
            boot.daa.min_level.max(1)
        };
        if let Some(algo) = parse_pow_algo_env() {
            boot.pow = algo;
        }
    } else {
        if std::env::var("AGORA_TEMPLATE_BITS").is_ok() {
            warn!(
                network = %params.network,
                bits = params.bits,
                "AGORA_TEMPLATE_BITS ignored — using ChainParams.bits"
            );
        }
        if let Some(algo) = parse_pow_algo_env() {
            if algo != params.pow_algorithm {
                warn!(
                    network = %params.network,
                    ?algo,
                    canonical = ?params.pow_algorithm,
                    "AGORA_POW_ALGO ignored — using ChainParams.pow_algorithm"
                );
            }
        }
    }

    boot
}

fn admit_gossip_block(
    chain: &Arc<Mutex<ChainState>>,
    mempool: &Arc<Mutex<Mempool>>,
    block: Block,
) -> Result<Hash, AdmitError> {
    let id = {
        let mut guard = chain
            .lock()
            .map_err(|_| AdmitError::Storage("chain lock poisoned".into()))?;
        guard.admit_block(block.clone())?
    };
    if let Ok(mut pool) = mempool.lock() {
        pool.evict_for_block(&block);
    }
    Ok(id)
}

/// Soft relay filter for unsolicited gossip only (not IBD / getblock responses).
fn relay_drop_stale_parents(chain: &Arc<Mutex<ChainState>>, block: &Block) -> bool {
    let Ok(guard) = chain.lock() else {
        return false;
    };
    if guard.has_block(&block.id()).unwrap_or(false) {
        return false;
    }
    if guard.parent_age_ok_for_relay(block) {
        return false;
    }
    warn!(
        block = %block.id().to_hex(),
        "relay drop: parents lag virtual tip (consensus admit still allowed via IBD)"
    );
    true
}

fn missing_parents(chain: &Arc<Mutex<ChainState>>, block: &Block) -> Result<Vec<Hash>, AdmitError> {
    let guard = chain
        .lock()
        .map_err(|_| AdmitError::Storage("chain lock poisoned".into()))?;
    Ok(guard.missing_parents_of(block))
}

/// Admit a block; on missing parents park it and fetch ancestors. On success, drain orphans.
fn handle_incoming_block(
    chain: &Arc<Mutex<ChainState>>,
    mempool: &Arc<Mutex<Mempool>>,
    store: &StateStore,
    orphans: &mut OrphanPool,
    pending: &mut PendingFetches,
    net: &NetworkHandle,
    peer: PeerId,
    block: Block,
) {
    let block_id = block.id();
    match admit_gossip_block(chain, mempool, block.clone()) {
        Ok(id) => {
            score_peer(net, peer, true);
            info!(%peer, block = %id.to_hex(), "admitted block");
            let _ = delete_orphan(store, &id);
            let chain_ref = chain.clone();
            let mempool_ref = mempool.clone();
            let drained = drain_orphans_after(orphans, id, |child| {
                match admit_gossip_block(&chain_ref, &mempool_ref, child.clone()) {
                    Ok(cid) => {
                        let _ = delete_orphan(store, &cid);
                        info!(block = %cid.to_hex(), "admitted orphan");
                        Ok(cid)
                    }
                    Err(AdmitError::MissingParent(_)) => {
                        match missing_parents(&chain_ref, &child) {
                            Ok(missing) if !missing.is_empty() => {
                                let _ = store_orphan(store, &child);
                                Err(Some(missing))
                            }
                            Ok(_) => {
                                let _ = delete_orphan(store, &child.id());
                                Err(None)
                            }
                            Err(_) => {
                                let _ = delete_orphan(store, &child.id());
                                Err(None)
                            }
                        }
                    }
                    Err(err) => {
                        let _ = delete_orphan(store, &child.id());
                        warn!(error = %err, "rejected orphan");
                        Err(None)
                    }
                }
            });
            if drained.len() > 1 {
                info!(count = drained.len() - 1, "drained orphans after admit");
            }
        }
        Err(AdmitError::MissingParent(_)) => {
            let missing =
                missing_parents(chain, &block).unwrap_or_else(|_| block.header.parents.clone());
            if missing.is_empty() {
                score_peer(net, peer, false);
                warn!(%peer, block = %block_id.to_hex(), "missing parent race — rejecting");
                return;
            }
            if orphans.park(block.clone(), &missing, Some(peer)) {
                let _ = store_orphan(store, &block);
                info!(
                    %peer,
                    block = %block_id.to_hex(),
                    missing = missing.len(),
                    orphans = orphans.len(),
                    "parked orphan — fetching parents"
                );
            } else {
                warn!(
                    %peer,
                    block = %block_id.to_hex(),
                    "orphan pool full — dropping block"
                );
            }
            for parent in missing {
                request_block_if_missing(chain, pending, net, peer, parent);
            }
        }
        Err(AdmitError::Duplicate(_)) => {
            // Benign — already have it.
            pending.complete(&block_id);
        }
        Err(err) => {
            score_peer(net, peer, false);
            warn!(%peer, error = %err, "rejected block");
        }
    }
}

fn request_block_if_missing(
    chain: &Arc<Mutex<ChainState>>,
    pending: &mut PendingFetches,
    net: &NetworkHandle,
    peer: PeerId,
    hash: Hash,
) {
    request_block_if_missing_with_reason(chain, pending, net, peer, hash, FetchReason::Sync)
}

fn request_block_if_missing_with_reason(
    chain: &Arc<Mutex<ChainState>>,
    pending: &mut PendingFetches,
    net: &NetworkHandle,
    peer: PeerId,
    hash: Hash,
    reason: FetchReason,
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
    if pending.request_with_reason(hash, reason) {
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

fn kick_headers_sync(
    chain: &Arc<Mutex<ChainState>>,
    net: &NetworkHandle,
    peer: PeerId,
    header_sync: &mut HashSet<PeerId>,
) {
    if !header_sync.insert(peer) {
        return;
    }
    let locator = match chain.lock() {
        Ok(guard) => match guard.block_locator() {
            Ok(l) => l,
            Err(err) => {
                warn!(%peer, error = %err, "block locator failed");
                header_sync.remove(&peer);
                return;
            }
        },
        Err(_) => {
            header_sync.remove(&peer);
            return;
        }
    };
    let req = GetHeadersRequest::new(locator, MAX_HEADERS_PER_RESPONSE);
    if let Err(err) = net.request_headers(peer, req) {
        warn!(%peer, error = %err, "getheaders request failed");
        header_sync.remove(&peer);
    } else {
        info!(%peer, "headers-first IBD requested");
    }
}

fn process_headers_response(
    chain: &Arc<Mutex<ChainState>>,
    pending: &mut PendingFetches,
    net: &NetworkHandle,
    peer: PeerId,
    headers: Vec<agora_types::BlockHeader>,
    header_sync: &mut HashSet<PeerId>,
) {
    if headers.is_empty() {
        header_sync.remove(&peer);
        info!(%peer, "getheaders empty — peer not ahead");
        return;
    }
    if let Err(err) = validate_header_chain(&headers) {
        score_peer(net, peer, false);
        header_sync.remove(&peer);
        warn!(%peer, error = %err, "rejected getheaders chain");
        return;
    }

    let mut missing = 0usize;
    for header in &headers {
        let hash = header.hash();
        let have = chain
            .lock()
            .ok()
            .and_then(|g| g.has_block(&hash).ok())
            .unwrap_or(false);
        if !have {
            missing += 1;
            request_block_if_missing(chain, pending, net, peer, hash);
        }
    }
    score_peer(net, peer, true);
    info!(
        %peer,
        headers = headers.len(),
        missing,
        "getheaders batch — fetching bodies oldest-first"
    );

    // Continue until the peer reports no further headers.
    if headers.len() as u32 >= MAX_HEADERS_PER_RESPONSE {
        header_sync.remove(&peer);
        kick_headers_sync(chain, net, peer, header_sync);
    } else {
        header_sync.remove(&peer);
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
    let boot = resolve_boot_config(&chain_params);
    let pow_algo = boot.pow;
    let template_bits = boot.initial_bits;

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
    let identity_path = std::path::Path::new(&data_dir)
        .join("p2p")
        .join("identity.key");
    let identity = load_or_generate_identity(&identity_path).expect("p2p identity");
    info!(
        path = %identity_path.display(),
        peer = %identity.public().to_peer_id(),
        "p2p identity ready"
    );
    net_cfg = net_cfg.with_identity(identity);

    let store = Arc::new(StateStore::open(&data_dir).expect("open state store"));
    let storage = StoragePolicy::from_env().for_network(chain_params.network.as_str());
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
    let artifact = agora_state_machine::GenesisArtifact::from_params(&chain_params);
    let policy_hash = artifact
        .consensus
        .as_ref()
        .map(|c| c.canonical_hash())
        .or_else(|| Hash::from_hex(&artifact.consensus_policy_hash))
        .unwrap_or(Hash::ZERO);
    let net_fp = agora_p2p::network_fingerprint(
        chain_params.network.chain_id(),
        &genesis_hash,
        &policy_hash,
    );
    net_cfg = net_cfg.with_fingerprint(agora_p2p::fingerprint_topic_tag(&net_fp));
    info!(
        network = %chain_params.network,
        premine = %premine_address,
        genesis = %genesis_hash.to_hex(),
        fingerprint = %agora_p2p::fingerprint_topic_tag(&net_fp),
        "genesis ready"
    );

    let chain = Arc::new(Mutex::new(
        ChainState::bootstrap_with(store.clone(), genesis_hash, boot.clone(), storage)
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

    let rpc_bind = std::env::var("AGORA_RPC_BIND").unwrap_or_else(|_| "127.0.0.1:8545".into());
    let rpc_token = std::env::var("AGORA_RPC_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::sync::Arc::<str>::from);
    enforce_rpc_bind_policy(&rpc_bind, rpc_token.is_some());
    let allow_fund = chain_params.network != NetworkId::Mainnet
        && matches!(
            std::env::var("AGORA_RPC_ALLOW_FUND").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
        );
    if chain_params.network == NetworkId::Mainnet && std::env::var("AGORA_RPC_ALLOW_FUND").is_ok() {
        warn!("AGORA_RPC_ALLOW_FUND ignored on mainnet — fund RPC permanently disabled");
    }
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
    let rate_limit_per_minute = std::env::var("AGORA_RPC_RATE_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    let rpc_http = RpcHttpConfig {
        token: rpc_token.clone(),
        rate_limit_per_minute,
    };
    tokio::spawn(serve_rpc(rpc_bind.clone(), dispatcher.clone(), rpc_http));

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
        chain
            .lock()
            .map(|c| c.difficulty().as_bits())
            .unwrap_or(template_bits)
    );

    let net = handle.clone();
    let local_peer = handle.peer_id();
    let orphan_store = store.clone();
    let tx_auth = agora_state_machine::TxAuthContext {
        chain_id: chain_params.network.chain_id().into(),
        genesis: genesis_hash,
    };
    tokio::spawn(async move {
        let mut pending = PendingFetches::new(Duration::from_secs(30));
        let mut orphans = OrphanPool::new(Duration::from_secs(120), 1_024);
        match list_orphans(orphan_store.as_ref()) {
            Ok(persisted) if !persisted.is_empty() => {
                let n = persisted.len();
                orphans.restore_from_blocks(persisted, |parent| {
                    chain
                        .lock()
                        .ok()
                        .and_then(|g| g.has_header(parent).ok())
                        .unwrap_or(false)
                });
                info!(
                    loaded = n,
                    parked = orphans.len(),
                    "restored durable orphan pool"
                );
            }
            Ok(_) => {}
            Err(err) => warn!(error = %err, "failed to load durable orphans"),
        }
        let mut header_sync: HashSet<PeerId> = HashSet::new();
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
                    kick_headers_sync(&chain, &net, peer, &mut header_sync);
                }
                NetworkEvent::PeerDisconnected(peer) => {
                    let prev = connected_peers.load(Ordering::Relaxed);
                    let n = prev.saturating_sub(1);
                    connected_peers.store(n, Ordering::Relaxed);
                    header_sync.remove(&peer);
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
                NetworkEvent::GetHeadersRequest {
                    peer,
                    locator,
                    limit,
                    stop_hash,
                    request_id,
                } => {
                    let headers = chain
                        .lock()
                        .ok()
                        .and_then(|g| g.headers_after_locator(&locator, limit, stop_hash).ok())
                        .unwrap_or_default();
                    let n = headers.len();
                    if let Err(err) = net.respond_get_headers(request_id, headers) {
                        warn!(%peer, error = %err, "getheaders rr respond failed");
                    } else {
                        info!(%peer, headers = n, "served getheaders rr");
                    }
                }
                NetworkEvent::GetHeadersResponse { peer, headers } => {
                    process_headers_response(
                        &chain,
                        &mut pending,
                        &net,
                        peer,
                        headers,
                        &mut header_sync,
                    );
                }
                NetworkEvent::GetHeadersFailure { peer, error } => {
                    header_sync.remove(&peer);
                    warn!(%peer, %error, "getheaders rr failure");
                }
                NetworkEvent::GetBlockResponse { peer, hash, block } => match block {
                    Some(block) => {
                        let reason = pending.take_reason(&hash);
                        // Announce-triggered fetches apply the same parent-age soft
                        // filter as unsolicited gossip so peers cannot thrash RandomX
                        // epochs via announce→getblock of ancient low-difficulty tips.
                        // IBD / orphan Sync fetches skip this filter.
                        if reason == FetchReason::Announce
                            && relay_drop_stale_parents(&chain, &block)
                        {
                            continue;
                        }
                        handle_incoming_block(
                            &chain,
                            &mempool,
                            orphan_store.as_ref(),
                            &mut orphans,
                            &mut pending,
                            &net,
                            peer,
                            block,
                        );
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
                        if relay_drop_stale_parents(&chain, &block) {
                            let _ = topic;
                            continue;
                        }
                        handle_incoming_block(
                            &chain,
                            &mempool,
                            orphan_store.as_ref(),
                            &mut orphans,
                            &mut pending,
                            &net,
                            peer,
                            block,
                        );
                        let _ = topic;
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
                                if relay_drop_stale_parents(&chain, &block) {
                                    let _ = topic;
                                    continue;
                                }
                                handle_incoming_block(
                                    &chain,
                                    &mempool,
                                    orphan_store.as_ref(),
                                    &mut orphans,
                                    &mut pending,
                                    &net,
                                    peer,
                                    block,
                                );
                                let _ = topic;
                            }
                            Err(ReconstructError::MissingShortIds(n)) => {
                                info!(
                                    %peer,
                                    %topic,
                                    missing = n,
                                    hash = %hash.to_hex(),
                                    "compact miss — requesting full block"
                                );
                                request_block_if_missing_with_reason(
                                    &chain,
                                    &mut pending,
                                    &net,
                                    peer,
                                    hash,
                                    FetchReason::Announce,
                                );
                            }
                            Err(ReconstructError::TxRootMismatch) => {
                                warn!(
                                    %peer,
                                    %topic,
                                    hash = %hash.to_hex(),
                                    "compact tx_root mismatch — requesting full block"
                                );
                                request_block_if_missing_with_reason(
                                    &chain,
                                    &mut pending,
                                    &net,
                                    peer,
                                    hash,
                                    FetchReason::Announce,
                                );
                            }
                        }
                    }
                    NetworkMessage::BlockAnnounce { hash } => {
                        info!(%peer, %topic, announce = %hash.to_hex(), "block announce");
                        request_block_if_missing_with_reason(
                            &chain,
                            &mut pending,
                            &net,
                            peer,
                            hash,
                            FetchReason::Announce,
                        );
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
                        match admit_transaction(store.as_ref(), &mempool, tx, &tx_auth) {
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
