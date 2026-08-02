//! Agora full node process.
//!
//! Wires consensus, state, p2p, and RPC once each crate reaches its roadmap phase.

use agora_consensus::{EmissionSchedule, Ghostdag, GhostdagConfig};
use agora_p2p::NetworkConfig;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let ghostdag = Ghostdag::new(GhostdagConfig::default());
    let emission = EmissionSchedule::default();
    let net = NetworkConfig::default();

    info!(
        k = ghostdag.config().k,
        initial_reward = emission.initial_reward,
        listen = %net.listen_addr,
        "agora-node foundation boot ok"
    );

    println!("Agora Network node (foundation scaffold) — see AGORA_MASTER_EXECUTION_ROADMAP.md");
}
