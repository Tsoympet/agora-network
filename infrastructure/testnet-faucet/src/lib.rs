//! Rate-limited Agora testnet faucet.

mod error;
mod faucet;
mod http;
pub mod node_rpc;

pub use error::{FaucetError, Result};
pub use faucet::{FaucetConfig, FaucetService, FundingTarget};
pub use http::serve;
