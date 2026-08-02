//! Rate-limited Agora testnet faucet.

mod error;
mod faucet;
mod http;

pub use error::{FaucetError, Result};
pub use faucet::{FaucetConfig, FaucetService};
pub use http::serve;
