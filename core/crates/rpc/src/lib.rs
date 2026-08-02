//! External access surface for wallets, explorer, and CEX gateways.

mod backend;
mod dispatch;
mod error;
mod methods;

pub use backend::{InMemoryBackend, RpcBackend, UtxoEntry};
pub use dispatch::RpcDispatcher;
pub use error::RpcError;
pub use methods::{RpcErrorBody, RpcMethod, RpcRequest, RpcResponse};
