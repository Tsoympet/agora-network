//! External access surface for wallets, explorer, and CEX gateways.

mod error;
mod methods;

pub use error::RpcError;
pub use methods::{RpcMethod, RpcRequest, RpcResponse};
