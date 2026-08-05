//! Agora full node process.
//!
//! Boots genesis through the transaction acceptance layer, binds the datadir to
//! the network fingerprint, gossips on fingerprint-scoped topics, and exposes a
//! minimal JSON-RPC line protocol for explorer/wallet queries.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agora_p2p::{Mempool, NetworkConfig, NetworkEvent, NetworkNode};
use agora_rpc::{dispatch, RpcBackend, RpcRequest};
use agora_state_machine::{
    assert_datadir_fingerprint, balance_of, load_acceptance_bitmap, load_acceptance_summary,
    load_network_fingerprint, meta_keys, tx_confirmation, ColumnFamily, GenesisBuilder, StateStore,
    StoreUtxoView,
};
use agora_types::{Address, Amount, Block, Hash, NetworkFingerprint, Transaction, TxConfirmation};
use borsh::BorshDeserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tracing::{info, warn};

/// Devnet treasury placeholder — override with `AGORA_PREMINE_ADDRESS` (40 hex chars).
const DEFAULT_PREMINE: Address = Address([0xC5; 20]);

struct NodeRpc {
    store: Arc<StateStore>,
    fingerprint: NetworkFingerprint,
    mempool: Arc<Mutex<Mempool>>,
    tip_blue_score: u64,
}

impl RpcBackend for NodeRpc {
    fn dag_tips(&self) -> Vec<Hash> {
        self.store
            .get_cf(ColumnFamily::Meta, meta_keys::TIPS)
            .ok()
            .flatten()
            .and_then(|bytes| borsh::from_slice::<Vec<Hash>>(&bytes).ok())
            .unwrap_or_default()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        let bytes = self
            .store
            .get_cf(ColumnFamily::Archival, hash.as_bytes())
            .ok()
            .flatten()?;
        Block::try_from_slice(&bytes).ok()
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, String> {
        let view = StoreUtxoView::new(&self.store);
        let mut pool = self.mempool.lock().map_err(|e| e.to_string())?;
        pool.admit(tx, &self.fingerprint, &view, self.tip_blue_score)
            .map_err(|e| e.to_string())
    }

    fn get_balance(&self, address: &Address) -> Amount {
        balance_of(&self.store, address).unwrap_or(Amount::ZERO)
    }

    fn get_block_acceptance(
        &self,
        hash: &Hash,
    ) -> Option<(agora_types::AcceptanceBitmap, u64, u64)> {
        let bitmap = load_acceptance_bitmap(&self.store, hash).ok().flatten()?;
        let (fees, reward) = load_acceptance_summary(&self.store, hash).ok().flatten()?;
        Some((bitmap, fees.as_base_units(), reward.as_base_units()))
    }

    fn get_tx_confirmation(&self, tx_id: &Hash, tip_blue_score: u64) -> TxConfirmation {
        tx_confirmation(&self.store, tx_id, tip_blue_score)
            .unwrap_or_else(|_| TxConfirmation::pending())
    }

    fn tip_blue_score(&self) -> u64 {
        self.tip_blue_score
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let listen = std::env::var("AGORA_LISTEN").unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/16111".into());
    let rpc_bind = std::env::var("AGORA_RPC_BIND").unwrap_or_else(|_| "127.0.0.1:18545".into());
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
    let store = if std::env::var("AGORA_STORE").as_deref() == Ok("memory") {
        warn!("AGORA_STORE=memory — datadir is ephemeral");
        StateStore::open_in_memory().expect("open memory store")
    } else {
        StateStore::open(&data_dir).expect("open rocksdb state store")
    };
    let store = Arc::new(store);

    let premine_address = std::env::var("AGORA_PREMINE_ADDRESS")
        .ok()
        .and_then(|hex| {
            let bytes = hex::decode(hex.trim()).ok()?;
            if bytes.len() != 20 {
                return None;
            }
            let mut arr = [0u8; 20];
            arr.copy_from_slice(&bytes);
            Some(Address(arr))
        })
        .unwrap_or(DEFAULT_PREMINE);

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
            .with_premine_address(premine_address)
            .ignite(&store)
            .expect("genesis ignition"),
    };

    let identity_path = PathBuf::from(&data_dir).join("libp2p.key");
    let net_cfg = NetworkConfig::default()
        .with_listen(listen)
        .with_bootstrap(bootstrap.clone())
        .with_fingerprint(fingerprint.clone())
        .with_identity_path(identity_path)
        .with_max_peers(64);

    let (handle, mut events, node) = NetworkNode::build(&net_cfg).expect("p2p build");
    for peer in &bootstrap {
        if let Err(err) = handle.dial(peer) {
            warn!(error = %err, peer, "bootstrap dial failed");
        }
    }

    let mempool = Arc::new(Mutex::new(Mempool::default()));
    let tip_blue_score = 1u64; // genesis blue score; advances when block apply is wired

    info!(
        peer_id = %handle.peer_id(),
        genesis = %genesis_hash.to_hex(),
        fingerprint = %fingerprint.digest_hex(),
        rpc = %rpc_bind,
        "agora-node boot ok"
    );
    println!(
        "Agora Network node — peer {} genesis {} fingerprint {} rpc {}",
        handle.peer_id(),
        genesis_hash.to_hex(),
        fingerprint.digest_hex(),
        rpc_bind
    );

    // JSON-RPC line protocol: one JSON request per line → one JSON response per line.
    let rpc_store = store.clone();
    let rpc_fp = fingerprint.clone();
    let rpc_pool = mempool.clone();
    tokio::spawn(async move {
        let listener = TcpListener::bind(&rpc_bind).await.expect("rpc bind");
        info!(%rpc_bind, "rpc listening");
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                continue;
            };
            let store = rpc_store.clone();
            let fingerprint = rpc_fp.clone();
            let mempool = rpc_pool.clone();
            tokio::spawn(async move {
                let (reader, mut writer) = socket.into_split();
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut backend = NodeRpc {
                        store: store.clone(),
                        fingerprint: fingerprint.clone(),
                        mempool: mempool.clone(),
                        tip_blue_score,
                    };
                    let response = match serde_json::from_str::<RpcRequest>(&line) {
                        Ok(req) => match dispatch(&req, &mut backend) {
                            Ok(resp) => serde_json::to_string(&resp)
                                .unwrap_or_else(|_| "{\"result\":null}".into()),
                            Err(err) => {
                                format!("{{\"error\":\"{}\"}}", err.to_string().replace('"', "'"))
                            }
                        },
                        Err(err) => format!(
                            "{{\"error\":\"invalid json: {}\"}}",
                            err.to_string().replace('"', "'")
                        ),
                    };
                    let _ = writer.write_all(format!("{response}\n").as_bytes()).await;
                }
            });
        }
    });

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
