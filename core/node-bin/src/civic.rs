//! Load / save civic governance snapshot from Meta CF.
//!
//! This snapshot is **local-admin**, not a consensus lane. `CivicSnapshot` is
//! JSON, unsigned, and excluded from `governance_treasury_root`. Vote RPCs
//! accept caller-supplied `raw_balance` / `total_supply` rather than a
//! network-bound secp256k1 envelope over live TLT UTXO weight. Do not gossip
//! or apply these blobs as BlockDAG mutations.

use agora_governance::{CivicSnapshot, CIVIC_META_KEY};
use agora_rpc::RpcError;
use agora_state_machine::{meta_keys, ColumnFamily, StateStore};

pub fn load_civic(store: &StateStore, eligible_power: u64) -> Result<CivicSnapshot, RpcError> {
    let key = if store
        .get_cf(ColumnFamily::Meta, meta_keys::GOVERNANCE)
        .map_err(|e| RpcError::Internal(e.to_string()))?
        .is_some()
    {
        meta_keys::GOVERNANCE
    } else {
        CIVIC_META_KEY
    };
    match store
        .get_cf(ColumnFamily::Meta, key)
        .map_err(|e| RpcError::Internal(e.to_string()))?
    {
        Some(bytes) => CivicSnapshot::from_json_bytes(&bytes)
            .map_err(|e| RpcError::Internal(format!("civic decode: {e}"))),
        None => Ok(CivicSnapshot::genesis(eligible_power)),
    }
}

pub fn save_civic(store: &StateStore, snap: &CivicSnapshot) -> Result<(), RpcError> {
    let bytes = snap
        .to_json_bytes()
        .map_err(|e| RpcError::Internal(format!("civic encode: {e}")))?;
    store
        .put_cf(ColumnFamily::Meta, meta_keys::GOVERNANCE, &bytes)
        .map_err(|e| RpcError::Internal(e.to_string()))
}

pub fn map_gov_err(err: agora_governance::GovernanceError) -> RpcError {
    RpcError::Rejected(err.to_string())
}
