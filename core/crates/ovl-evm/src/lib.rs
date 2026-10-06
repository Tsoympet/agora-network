//! `OVL-EVM-v1`: pinned Shanghai execution for native OVL.
//!
//! Maturity: Experimental. The dev gate is inactive until a genesis explicitly
//! activates it. This crate does not decide Trident finality.

mod error;
mod exec;
mod fee;
mod index;
mod rpc;
mod tx;
mod world;

pub use error::EvmError;
pub use exec::{apply_raw_transaction, estimate_gas, eth_call, measure_shanghai_gas};
pub use fee::{effective_gas_price, next_base_fee};
pub use index::{index_receipts, topic};
pub use rpc::{dispatch, dispatch_with_view, EthNodeView, EthSyncStatus, PendingTx};
pub use tx::{
    dev_signing_key, ethereum_address_from_signing_key, parse_raw_transaction, sign_eip1559,
    sign_eip2930, sign_legacy, AccessItem, ParsedTx,
};
pub use world::{EvmReceipt, OvlEvmWorld, SelectedBlock, EXECUTION_SUBROOT_DOMAIN};

pub const PROFILE_JSON: &str = include_str!("../profile/ovl-evm-v1.json");

/// DRC and TLT have no path into this executor.
pub fn reject_foreign_native_asset(asset: &str) -> Result<(), EvmError> {
    if asset.eq_ignore_ascii_case("OVL") {
        return Ok(());
    }
    Err(EvmError::rejected(format!(
        "{asset} cannot enter the OVL EVM"
    )))
}

#[cfg(test)]
mod tests;
