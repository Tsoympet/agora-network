//! Testnet faucet HTTP service.
//!
//! Env:
//! - `AGORA_FAUCET_BIND` (default `127.0.0.1:18081`)
//! - `AGORA_FAUCET_DRIP` base units per drip (default `1000000000` = 10 AGORA)
//! - `AGORA_FAUCET_COOLDOWN_SECS` (default `60`)
//! - `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`) — live node with
//!   `AGORA_RPC_ALLOW_FUND=1` so drips mint spendable `cf_utxo` outputs

use std::sync::Arc;
use std::time::Duration;

use agora_testnet_faucet::{serve, FaucetConfig, FaucetService};
use agora_types::Amount;
use tokio::sync::Mutex;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let bind = std::env::var("AGORA_FAUCET_BIND").unwrap_or_else(|_| "127.0.0.1:18081".into());
    let drip = std::env::var("AGORA_FAUCET_DRIP")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1_000_000_000);
    let cooldown = std::env::var("AGORA_FAUCET_COOLDOWN_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    let rpc_url =
        std::env::var("AGORA_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545/rpc".into());

    let config = FaucetConfig {
        drip_amount: Amount::from_base_units(drip),
        cooldown: Duration::from_secs(cooldown),
        max_total: None,
    };
    info!(
        drip_base_units = drip,
        cooldown_secs = cooldown,
        %rpc_url,
        "faucet policy → live node UTXO mints"
    );

    let faucet = Arc::new(Mutex::new(FaucetService::node(config, rpc_url)));
    serve(&bind, faucet).await;
}
