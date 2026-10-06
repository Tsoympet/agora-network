//! External access surface for wallets, explorer, and CEX gateways.

mod backend;
mod dispatch;
mod drc_trust_line_params;
mod error;
mod methods;

pub use drc_trust_line_params::{
    parse_holder_issuer_asset, parse_issued_asset_id, parse_issued_currency_code,
};

pub use backend::{
    AccountBalances, DrcDepositPreauthStatus, FeeEstimate, InMemoryBackend, MempoolEntry, NodeInfo,
    RpcBackend, TltCovenantLookup, TxLookup, TxStatus, UtxoEntry,
};
pub use dispatch::RpcDispatcher;
pub use error::RpcError;
pub use methods::{RpcErrorBody, RpcMethod, RpcRequest, RpcResponse};
