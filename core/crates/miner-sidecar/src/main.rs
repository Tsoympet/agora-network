//! RandomX CPU miner sidecar.
//!
//! Runs out-of-process from the full node so mining never blocks consensus I/O.

use tracing::info;

#[tokio::main]
async fn main() {
    info!("agora-miner sidecar starting (RandomX template loop not yet wired)");
    println!("agora-miner: Phase 6 will poll node templates and search RandomX nonces");
}
