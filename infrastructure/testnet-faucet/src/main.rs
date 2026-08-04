//! Testnet faucet HTTP service.
//!
//! Env:
//! - `AGORA_FAUCET_BIND` (default `127.0.0.1:18081`)
//! - `AGORA_FAUCET_DRIP` base units per drip (default `1000000000` = 10 AGORA)
//! - `AGORA_FAUCET_COOLDOWN_SECS` (default `60`)
//! - `AGORA_FAUCET_MAX_TOTAL` optional hard cap on total base units dispensed
//! - `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`)
//! - `AGORA_FAUCET_MODE` — `treasury` (default, signed spends) or `mint` (lab `agora_fundAddress`)
//! - `AGORA_FAUCET_MNEMONIC` — BIP-39 for treasury spends (default: testnet premine abandon phrase)
//! - `AGORA_RPC_TOKEN` — bearer token when the node requires auth

use std::sync::Arc;
use std::time::Duration;

use agora_testnet_faucet::{serve, FaucetConfig, FaucetService, TESTNET_TREASURY_MNEMONIC};
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

    let max_total = std::env::var("AGORA_FAUCET_MAX_TOTAL")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Amount::from_base_units);
    let config = FaucetConfig {
        drip_amount: Amount::from_base_units(drip),
        cooldown: Duration::from_secs(cooldown),
        max_total,
    };

    let mode = std::env::var("AGORA_FAUCET_MODE").unwrap_or_else(|_| "treasury".into());
    let faucet = match mode.to_ascii_lowercase().as_str() {
        "mint" | "fund" | "node" => {
            info!(
                drip_base_units = drip,
                cooldown_secs = cooldown,
                max_total = ?max_total.map(|a| a.as_base_units()),
                %rpc_url,
                "faucet policy → live node UTXO mints (lab only)"
            );
            FaucetService::node(config, rpc_url)
        }
        _ => {
            let mnemonic = std::env::var("AGORA_FAUCET_MNEMONIC")
                .unwrap_or_else(|_| TESTNET_TREASURY_MNEMONIC.into());
            info!(
                drip_base_units = drip,
                cooldown_secs = cooldown,
                max_total = ?max_total.map(|a| a.as_base_units()),
                %rpc_url,
                "faucet policy → treasury signed spends (non-mint)"
            );
            FaucetService::treasury(config, rpc_url, mnemonic)
        }
    };

    let faucet = Arc::new(Mutex::new(faucet));
    serve(&bind, faucet).await;
}
