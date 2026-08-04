//! Rate-limited Agora testnet faucet.

mod error;
mod faucet;
mod http;
pub mod node_rpc;
mod treasury;

pub use error::{FaucetError, Result};
pub use faucet::{FaucetConfig, FaucetService, FundingTarget, TESTNET_TREASURY_MNEMONIC};
pub use http::serve;
