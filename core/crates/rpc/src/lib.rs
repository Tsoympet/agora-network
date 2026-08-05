//! External access surface for wallets, explorer, and CEX gateways.
//!
//! Confirmation and inclusion status are defined by the transaction acceptance
//! layer — never by block color alone. The explorer consumes these RPC views.

mod error;
mod methods;

pub use error::RpcError;
pub use methods::{BlockAcceptanceView, RpcMethod, RpcRequest, RpcResponse, TxAcceptanceView};
