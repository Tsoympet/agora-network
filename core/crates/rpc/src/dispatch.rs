use agora_types::{Address, Amount, Block, Hash, Transaction};
use serde_json::{json, Value};

use crate::backend::RpcBackend;
use crate::error::RpcError;
use crate::methods::{RpcMethod, RpcRequest, RpcResponse};

/// Dispatches JSON-RPC style requests against an [`RpcBackend`].
#[derive(Debug)]
pub struct RpcDispatcher<B: RpcBackend> {
    backend: B,
}

impl<B: RpcBackend> RpcDispatcher<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn handle(&mut self, req: RpcRequest) -> RpcResponse {
        let id = req.id.clone();
        match self.dispatch(&req) {
            Ok(result) => RpcResponse::ok(id, result),
            Err(err) => RpcResponse::err(id, &err),
        }
    }

    fn dispatch(&mut self, req: &RpcRequest) -> Result<Value, RpcError> {
        let method = RpcMethod::parse(&req.method)
            .ok_or_else(|| RpcError::MethodNotFound(req.method.clone()))?;
        match method {
            RpcMethod::GetDagTips => {
                let tips: Vec<String> = self
                    .backend
                    .dag_tips()
                    .into_iter()
                    .map(|h| h.to_hex())
                    .collect();
                Ok(json!(tips))
            }
            RpcMethod::GetBlock => {
                let hash = param_hash(&req.params, "hash")?;
                let block = self
                    .backend
                    .get_block(&hash)
                    .ok_or_else(|| RpcError::NotFound(hash.to_hex()))?;
                Ok(block_to_explorer_json(&block))
            }
            RpcMethod::GetTransaction => {
                let tx_id = param_hash(&req.params, "tx_id")?;
                let lookup = self.backend.get_transaction(&tx_id)?;
                Ok(tx_lookup_to_json(&lookup))
            }
            RpcMethod::GetMempool => {
                let limit = optional_limit(&req.params, 128)?;
                let entries = self.backend.get_mempool(limit)?;
                Ok(json!({
                    "count": entries.len(),
                    "transactions": entries.iter().map(mempool_entry_to_json).collect::<Vec<_>>(),
                }))
            }
            RpcMethod::SubmitTransaction => {
                let raw = tx_param(&req.params)?;
                let tx: Transaction = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_transaction(tx)?;
                Ok(json!({ "tx_id": id.to_hex() }))
            }
            RpcMethod::GetBalance => {
                let address = param_address(&req.params, "address")?;
                let bal = self.backend.get_balance(&address);
                Ok(json!({
                    "address": address.to_bech32(),
                    "balance": bal.as_base_units(),
                }))
            }
            RpcMethod::GetUtxos => {
                let address = param_address(&req.params, "address")?;
                let utxos = self.backend.get_utxos(&address)?;
                Ok(json!({
                    "address": address.to_bech32(),
                    "utxos": utxos.iter().map(|u| json!({
                        "tx_id": u.outpoint.tx_id.to_hex(),
                        "index": u.outpoint.index,
                        "value": u.value.as_base_units(),
                    })).collect::<Vec<_>>(),
                }))
            }
            RpcMethod::FundAddress => {
                let address = param_address(&req.params, "address")?;
                let amount = param_amount(&req.params, "amount")?;
                let bal = self.backend.fund_address(address, amount)?;
                Ok(json!({
                    "address": address.to_bech32(),
                    "balance": bal.as_base_units(),
                }))
            }
            RpcMethod::GetBlockTemplate => {
                let block = self.backend.get_block_template()?;
                // Keep native serde shape (`Hash` as byte arrays) for miner-sidecar.
                Ok(serde_json::to_value(block)
                    .map_err(|e| RpcError::Internal(e.to_string()))?)
            }
            RpcMethod::SubmitBlock => {
                let raw = block_param(&req.params)?;
                let block: Block = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_block(block)?;
                Ok(json!({ "block_id": id.to_hex() }))
            }
        }
    }
}

/// Hex-friendly block JSON for wallets / explorer (hashes as hex strings).
fn block_to_explorer_json(block: &Block) -> Value {
    let id = block.id().to_hex();
    let transactions: Vec<Value> = block
        .transactions
        .iter()
        .map(tx_to_explorer_json)
        .collect();
    json!({
        "id": id,
        "header": header_to_explorer_json(&block.header, Some(id)),
        "tx_count": block.transactions.len(),
        "transactions": transactions,
    })
}

fn tx_to_explorer_json(tx: &Transaction) -> Value {
    json!({
        "tx_id": tx.tx_id().to_hex(),
        "version": tx.version,
        "inputs": tx.inputs.iter().map(|i| json!({
            "tx_id": i.previous_outpoint.tx_id.to_hex(),
            "index": i.previous_outpoint.index,
        })).collect::<Vec<_>>(),
        "outputs": tx.outputs.iter().map(|o| json!({
            "value": o.value.as_base_units(),
            "address": o.address.to_bech32(),
        })).collect::<Vec<_>>(),
        "nonce": tx.nonce,
        "is_coinbase": tx.inputs.is_empty(),
    })
}

fn tx_lookup_to_json(lookup: &crate::backend::TxLookup) -> Value {
    json!({
        "tx_id": lookup.tx_id.to_hex(),
        "status": lookup.status.as_str(),
        "block_id": lookup.block_id.map(|h| h.to_hex()),
        "index": lookup.index,
        "fee": lookup.fee,
        "transaction": lookup.transaction.as_ref().map(tx_to_explorer_json),
    })
}

fn mempool_entry_to_json(entry: &crate::backend::MempoolEntry) -> Value {
    json!({
        "tx_id": entry.tx_id.to_hex(),
        "fee": entry.fee,
        "transaction": tx_to_explorer_json(&entry.transaction),
    })
}

/// Optional `{ "limit": N }` / `[N]` / bare number; default when omitted.
fn optional_limit(params: &Value, default: usize) -> Result<usize, RpcError> {
    if params.is_null() {
        return Ok(default);
    }
    if let Some(arr) = params.as_array() {
        if arr.is_empty() {
            return Ok(default);
        }
        return parse_limit_value(&arr[0], default);
    }
    if let Some(obj) = params.as_object() {
        if obj.is_empty() {
            return Ok(default);
        }
        if let Some(v) = obj.get("limit") {
            return parse_limit_value(v, default);
        }
        return Ok(default);
    }
    parse_limit_value(params, default)
}

fn parse_limit_value(v: &Value, default: usize) -> Result<usize, RpcError> {
    if v.is_null() {
        return Ok(default);
    }
    let n = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| RpcError::InvalidParams("`limit` must be u64".into()))?;
    if n == 0 {
        return Ok(default);
    }
    Ok(n.min(10_000) as usize)
}

fn header_to_explorer_json(header: &agora_types::BlockHeader, id: Option<String>) -> Value {
    let mut obj = json!({
        "version": header.version,
        "parents": header.parents.iter().map(|p| p.to_hex()).collect::<Vec<_>>(),
        "timestamp_ms": header.timestamp_ms,
        "bits": header.bits,
        "nonce": header.nonce,
        "tx_root": header.tx_root.to_hex(),
    });
    if let Some(id) = id {
        obj.as_object_mut()
            .expect("object")
            .insert("id".into(), json!(id));
    }
    obj
}

fn single_or_named(params: &Value, key: &str) -> Result<Value, RpcError> {
    if let Some(obj) = params.as_object() {
        return obj
            .get(key)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")));
    }
    if let Some(arr) = params.as_array() {
        return arr
            .first()
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")));
    }
    // Allow bare value.
    if !params.is_null() {
        return Ok(params.clone());
    }
    Err(RpcError::InvalidParams(format!("missing `{key}`")))
}

/// Accept `{ "tx": {...} }`, `[tx]`, or a bare transaction object.
fn tx_param(params: &Value) -> Result<Value, RpcError> {
    if let Some(obj) = params.as_object() {
        if let Some(tx) = obj.get("tx") {
            return Ok(tx.clone());
        }
        if obj.contains_key("version") && obj.contains_key("outputs") {
            return Ok(params.clone());
        }
        return Err(RpcError::InvalidParams("missing `tx`".into()));
    }
    single_or_named(params, "tx")
}

/// Accept `{ "block": {...} }`, `[block]`, or a bare block object.
fn block_param(params: &Value) -> Result<Value, RpcError> {
    if let Some(obj) = params.as_object() {
        if let Some(block) = obj.get("block") {
            return Ok(block.clone());
        }
        if obj.contains_key("header") {
            return Ok(params.clone());
        }
        return Err(RpcError::InvalidParams("missing `block`".into()));
    }
    single_or_named(params, "block")
}

fn param_hash(params: &Value, key: &str) -> Result<Hash, RpcError> {
    let v = single_or_named(params, key)?;
    let s = v
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be hex string")))?;
    Hash::from_hex(s).ok_or_else(|| RpcError::InvalidParams(format!("invalid hash `{s}`")))
}

fn param_address(params: &Value, key: &str) -> Result<Address, RpcError> {
    let v = single_or_named(params, key)?;
    let s = v
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be bech32 or hex string")))?;
    Address::parse(s).ok_or_else(|| RpcError::InvalidParams(format!("invalid address `{s}`")))
}

fn param_amount(params: &Value, key: &str) -> Result<Amount, RpcError> {
    // Support object `{address, amount}` or array `[address, amount]`.
    let amount_val = if let Some(obj) = params.as_object() {
        obj.get(key)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")))?
    } else if let Some(arr) = params.as_array() {
        arr.get(1)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")))?
    } else {
        return Err(RpcError::InvalidParams(format!("missing `{key}`")));
    };

    let units = amount_val
        .as_u64()
        .or_else(|| amount_val.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be u64")))?;
    Ok(Amount::from_base_units(units))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::InMemoryBackend;
    use agora_types::{Block, BlockHeader, TxOut};

    #[test]
    fn tips_balance_submit_fund() {
        let mut backend = InMemoryBackend::new();
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
        };
        let genesis_id = genesis.id();
        backend.insert_block(genesis);

        let mut rpc = RpcDispatcher::new(backend);
        let tips = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDagTips".into(),
            params: json!([]),
        });
        assert_eq!(
            tips.result.unwrap(),
            json!([genesis_id.to_hex()])
        );

        let addr = Address::from_hex("aabbccddeeff00112233445566778899aabbccdd").unwrap();
        let funded = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_fundAddress".into(),
            params: json!({"address": addr.to_bech32(), "amount": 500u64}),
        });
        let funded_res = funded.result.unwrap();
        assert_eq!(funded_res["balance"], json!(500));
        assert_eq!(funded_res["address"], json!(addr.to_bech32()));

        let bal = rpc.handle(RpcRequest {
            id: None,
            method: "agora_getBalance".into(),
            params: json!([addr.to_hex()]), // hex still accepted
        });
        let bal_res = bal.result.unwrap();
        assert_eq!(bal_res["balance"], json!(500));
        assert_eq!(bal_res["address"], json!(addr.to_bech32()));

        let utxos = rpc.handle(RpcRequest {
            id: Some(json!(21)),
            method: "agora_getUtxos".into(),
            params: json!({"address": addr.to_bech32()}),
        });
        let utxo_list = utxos.result.unwrap()["utxos"].as_array().unwrap().clone();
        assert_eq!(utxo_list.len(), 1);
        assert_eq!(utxo_list[0]["value"], json!(500));
        assert_eq!(utxo_list[0]["index"], json!(0));

        let tx = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: addr,
            }],
            1,
        );
        let submitted = rpc.handle(RpcRequest {
            id: Some(json!(3)),
            method: "agora_submitTransaction".into(),
            params: json!(tx.clone()),
        });
        let tx_id = submitted.result.unwrap()["tx_id"].as_str().unwrap().to_string();

        let pending = rpc.handle(RpcRequest {
            id: Some(json!(31)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": tx_id}),
        });
        let pending_res = pending.result.unwrap();
        assert_eq!(pending_res["status"], json!("pending"));
        assert!(pending_res["transaction"].is_object());

        let pool = rpc.handle(RpcRequest {
            id: Some(json!(311)),
            method: "agora_getMempool".into(),
            params: json!([]),
        });
        let pool_res = pool.result.unwrap();
        assert_eq!(pool_res["count"], json!(1));
        assert_eq!(pool_res["transactions"][0]["tx_id"], json!(tx_id));

        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(32)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": Hash::ZERO.to_hex()}),
        });
        assert_eq!(unknown.result.unwrap()["status"], json!("unknown"));

        let mined = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_id],
                timestamp_ms: 1,
                bits: 0,
                nonce: 1,
                tx_root: Block::compute_tx_root(std::slice::from_ref(&tx)),
            },
            transactions: vec![tx],
        };
        let mined_id = mined.id();
        rpc.backend_mut().insert_block(mined);

        let confirmed = rpc.handle(RpcRequest {
            id: Some(json!(33)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": tx_id}),
        });
        let confirmed_res = confirmed.result.unwrap();
        assert_eq!(confirmed_res["status"], json!("confirmed"));
        assert_eq!(confirmed_res["block_id"], json!(mined_id.to_hex()));
        assert_eq!(confirmed_res["index"], json!(0));

        let block = rpc.handle(RpcRequest {
            id: Some(json!(4)),
            method: "agora_getBlock".into(),
            params: json!([genesis_id.to_hex()]),
        });
        let result = block.result.unwrap();
        assert_eq!(result["id"], json!(genesis_id.to_hex()));
        assert!(result["header"]["parents"].as_array().unwrap().is_empty());
        assert_eq!(result["tx_count"], json!(0));
        assert!(result["transactions"].as_array().unwrap().is_empty());
    }
}
