//! Canonical light-client RPC payloads.
//!
//! Headers, body bindings, and account balances come from the live chain and
//! state store. This module does not invent proofs: a missing body, a header
//! whose `tx_root` does not match the binding, or an uncolored selected parent
//! before genesis is an error.

use agora_rpc::RpcError;
use agora_state_machine::{account_exists, load_account, StateStore};
use agora_types::{
    fold_body_binding, prove_tlt_tx_merkle, tlt_body_binding, Block, Hash, NativeAssetId,
    TltTxMerkleProof,
};
use serde_json::{json, Value};

use crate::admit::ChainState;

const MAX_LIGHT_HEADERS: usize = 512;

fn hex_hashes(ids: &[Hash]) -> Vec<String> {
    ids.iter().map(Hash::to_hex).collect()
}

fn step_json(step: &agora_types::BodyBindingStep) -> Value {
    match step {
        agora_types::BodyBindingStep::V2 {
            account_ids,
            stake_ids,
        } => json!({
            "kind": "v2",
            "account_ids": hex_hashes(account_ids),
            "stake_ids": hex_hashes(stake_ids),
        }),
        agora_types::BodyBindingStep::V3 { execution_ids } => json!({
            "kind": "v3",
            "execution_ids": hex_hashes(execution_ids),
        }),
        agora_types::BodyBindingStep::V4 { payment_ids } => json!({
            "kind": "v4",
            "payment_ids": hex_hashes(payment_ids),
        }),
        agora_types::BodyBindingStep::Versioned {
            domain,
            version,
            id_lists,
        } => json!({
            "kind": "versioned",
            "domain": domain,
            "version": version,
            "id_lists": id_lists.iter().map(|list| hex_hashes(list)).collect::<Vec<_>>(),
        }),
    }
}

fn header_json(header: &agora_types::BlockHeader) -> Value {
    json!({
        "version": header.version,
        "parents": hex_hashes(&header.parents),
        "timestamp_ms": header.timestamp_ms,
        "bits": header.bits,
        "nonce": header.nonce,
        "tx_root": header.tx_root.to_hex(),
    })
}

fn proof_json(proof: &TltTxMerkleProof) -> Value {
    json!({
        "index": proof.index,
        "tx_id": proof.tx_id.to_hex(),
        "siblings": hex_hashes(&proof.siblings),
    })
}

/// Selected-parent headers from `tip` toward genesis, newest first.
pub fn light_headers_json(
    chain: &ChainState,
    network: &str,
    genesis: Hash,
    tip: Option<Hash>,
    limit: usize,
) -> Result<Value, RpcError> {
    let tip = match tip {
        Some(hash) => hash,
        None => chain
            .virtual_tip()
            .map_err(|e| RpcError::Internal(e.to_string()))?,
    };
    let limit = limit.clamp(1, MAX_LIGHT_HEADERS);
    let mut headers = Vec::new();
    let mut cursor = tip;
    for _ in 0..limit {
        let header = chain
            .load_header(&cursor)
            .map_err(|e| RpcError::Internal(e.to_string()))?
            .ok_or_else(|| RpcError::NotFound(cursor.to_hex()))?;
        let hash = header.hash();
        if hash != cursor {
            return Err(RpcError::Internal(format!(
                "stored header {} does not hash to its key",
                cursor.to_hex()
            )));
        }
        let selected_parent = chain.selected_parent_of(&cursor);
        let blue_score = chain.blue_score_of(&cursor).unwrap_or(0);
        let is_genesis = cursor == genesis;
        headers.push(json!({
            "hash": hash.to_hex(),
            "selected_parent": selected_parent.map(|parent| parent.to_hex()),
            "blue_score": blue_score,
            "is_genesis": is_genesis,
            "header": header_json(&header),
        }));
        if is_genesis {
            break;
        }
        let Some(parent) = selected_parent else {
            return Err(RpcError::Internal(format!(
                "selected parent missing for non-genesis header {}",
                cursor.to_hex()
            )));
        };
        cursor = parent;
    }
    let reaches_genesis = headers
        .last()
        .and_then(|row| row.get("is_genesis"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let finality = finality_json(chain, &tip)?;
    Ok(json!({
        "network": network,
        "genesis": genesis.to_hex(),
        "tip": tip.to_hex(),
        "order": "tip_to_genesis",
        "reaches_genesis": reaches_genesis,
        "pow_checked_by": "full_node",
        "headers": headers,
        "finality": finality,
    }))
}

fn finality_json(chain: &ChainState, block_hash: &Hash) -> Result<Value, RpcError> {
    let cert = chain
        .finality_certificate(block_hash)
        .map_err(|e| RpcError::Internal(e.to_string()))?;
    let finalized_tip = chain
        .finalized_blue_score()
        .map_err(|e| RpcError::Internal(e.to_string()))?
        .unwrap_or(0);
    match cert {
        Some(cert) => Ok(json!({
            "block_hash": cert.body.block_hash.to_hex(),
            "blue_score": cert.body.blue_score,
            "node_state": cert.state.as_str(),
            "pow_work_met": cert.pow_work_met,
            "ovl_signed_stake": cert.ovl_signed_stake.to_string(),
            "ovl_active_stake": cert.ovl_active_stake.to_string(),
            "drc_signed_stake": cert.drc_signed_stake.to_string(),
            "drc_active_stake": cert.drc_active_stake.to_string(),
            "stake_fields_present": true,
            "finalized_tip_blue_score": finalized_tip,
        })),
        None => Ok(json!({
            "block_hash": block_hash.to_hex(),
            "node_state": "Proposed",
            "pow_work_met": false,
            "ovl_signed_stake": "0",
            "ovl_active_stake": "0",
            "drc_signed_stake": "0",
            "drc_active_stake": "0",
            "stake_fields_present": false,
            "finalized_tip_blue_score": finalized_tip,
        })),
    }
}

/// Body-root binding for one stored block. Fails when the body is missing or
/// the stored header root does not match the recomputed binding.
pub fn block_binding_json(chain: &ChainState, hash: &Hash) -> Result<Value, RpcError> {
    let block = chain
        .load_block(hash)
        .map_err(|e| RpcError::Internal(e.to_string()))?
        .ok_or_else(|| RpcError::NotFound(hash.to_hex()))?;
    if block.id() != *hash {
        return Err(RpcError::Internal(
            "stored block id does not match the requested hash".into(),
        ));
    }
    binding_json(&block)
}

fn binding_json(block: &Block) -> Result<Value, RpcError> {
    let steps = tlt_body_binding(block);
    let tx_merkle_root = Block::compute_tx_root(&block.transactions);
    let folded = fold_body_binding(tx_merkle_root, &steps)
        .map_err(|_| RpcError::Internal("unsupported body binding".into()))?;
    if folded != block.header.tx_root {
        return Err(RpcError::Internal(
            "refusing block binding: header tx_root does not match the body".into(),
        ));
    }
    let ovl_execution_ids: Vec<String> = block
        .ovl_executions
        .iter()
        .map(|tx| tx.tx_id().to_hex())
        .collect();
    let drc_payment_ids: Vec<String> = block
        .drc_payments
        .iter()
        .map(|tx| tx.payment_id().to_hex())
        .collect();
    Ok(json!({
        "hash": block.id().to_hex(),
        "header": header_json(&block.header),
        "tx_merkle_root": tx_merkle_root.to_hex(),
        "utxo_only": steps.is_empty(),
        "binding": steps.iter().map(step_json).collect::<Vec<_>>(),
        "ovl_execution_ids": ovl_execution_ids,
        "drc_payment_ids": drc_payment_ids,
        "programmable_lane": "OVL",
        "drc_contract_lane": false,
    }))
}

/// TLT Merkle proof plus the body binding for the block that contains `tx_id`.
///
/// Pending and unknown transactions return `proof: null`. A confirmed or
/// orphaned transaction without a matching body is an error, not an empty proof.
pub fn tlt_inclusion_json(
    chain: &ChainState,
    tx_id: &Hash,
    status: &str,
    block_id: Option<Hash>,
    on_virtual_spine: bool,
) -> Result<Value, RpcError> {
    let Some(block_id) = block_id.filter(|_| status != "pending" && status != "unknown") else {
        return Ok(json!({
            "tx_id": tx_id.to_hex(),
            "status": status,
            "on_virtual_spine": false,
            "proof": Value::Null,
        }));
    };
    let block = chain
        .load_block(&block_id)
        .map_err(|e| RpcError::Internal(e.to_string()))?
        .ok_or_else(|| {
            RpcError::Internal(format!(
                "tx {} is indexed in missing block {}",
                tx_id.to_hex(),
                block_id.to_hex()
            ))
        })?;
    let index = block
        .transactions
        .iter()
        .position(|tx| tx.tx_id() == *tx_id)
        .ok_or_else(|| {
            RpcError::Internal(format!(
                "tx {} is not in the body of {}",
                tx_id.to_hex(),
                block_id.to_hex()
            ))
        })?;
    let tx_ids: Vec<Hash> = block.transactions.iter().map(|tx| tx.tx_id()).collect();
    let proof = prove_tlt_tx_merkle(&tx_ids, index)
        .ok_or_else(|| RpcError::Internal(format!("unable to prove tx {}", tx_id.to_hex())))?;
    let mut binding = binding_json(&block)?;
    if let Some(object) = binding.as_object_mut() {
        object.insert("inclusion".into(), proof_json(&proof));
        object.insert(
            "selected_parent".into(),
            json!(chain
                .selected_parent_of(&block_id)
                .map(|parent| parent.to_hex())),
        );
        object.insert(
            "blue_score".into(),
            json!(chain.blue_score_of(&block_id).unwrap_or(0)),
        );
    }
    Ok(json!({
        "tx_id": tx_id.to_hex(),
        "status": status,
        "on_virtual_spine": on_virtual_spine,
        "proof": binding,
    }))
}

/// Canonical TLT UTXO sum plus OVL and DRC account records.
///
/// Balances are full-node reads. `header_proven` stays false because account
/// totals are not committed inside `BlockHeader`.
pub fn native_balances_json(
    store: &StateStore,
    address: &agora_types::Address,
    address_bech32: String,
    tlt_balance: u64,
) -> Result<Value, RpcError> {
    let ovl = account_json(store, NativeAssetId::OVL, address)?;
    let drc = account_json(store, NativeAssetId::DRC, address)?;
    Ok(json!({
        "address": address_bech32,
        "address_hex": address.to_hex(),
        "assets": {
            "TLT": {
                "asset": "TLT",
                "module": "utxo",
                "balance": tlt_balance.to_string(),
                "header_proven": false,
            },
            "OVL": ovl,
            "DRC": drc,
        }
    }))
}

fn account_json(
    store: &StateStore,
    asset: NativeAssetId,
    address: &agora_types::Address,
) -> Result<Value, RpcError> {
    let exists =
        account_exists(store, asset, address).map_err(|e| RpcError::Internal(e.to_string()))?;
    let state =
        load_account(store, asset, address).map_err(|e| RpcError::Internal(e.to_string()))?;
    let ticker = match asset {
        NativeAssetId::OVL => "OVL",
        NativeAssetId::DRC => "DRC",
        NativeAssetId::TLT => {
            return Err(RpcError::Internal(
                "TLT balance is not an account record".into(),
            ))
        }
    };
    Ok(json!({
        "asset": ticker,
        "module": "account",
        "balance": state.balance.to_string(),
        "nonce": state.nonce.to_string(),
        "record": if exists { "present" } else { "absent" },
        "header_proven": false,
    }))
}
