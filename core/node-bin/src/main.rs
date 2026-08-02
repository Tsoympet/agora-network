//! Agora full node process.
//!
//! Wires consensus, state, p2p, and RPC once each crate reaches its roadmap phase.

use agora_consensus::{EmissionSchedule, Ghostdag, GhostdagConfig};
use agora_p2p::NetworkConfig;
use agora_state_machine::{GenesisBuilder, StateStore};
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let ghostdag = Ghostdag::new(GhostdagConfig::default());
    let emission = EmissionSchedule::default();
    let net = NetworkConfig::default();

    let store = StateStore::open("data/agora-node").expect("open state store");
    let genesis_hash = GenesisBuilder::default()
        .ignite(&store)
        .expect("genesis ignition");

    info!(
        k = ghostdag.config().k,
        initial_reward = emission.initial_reward,
        listen = %net.listen_addr,
        genesis = %genesis_hash.to_hex(),
        "agora-node foundation boot ok"
    );

    println!("Agora Network node — genesis {}", genesis_hash.to_hex());
}
