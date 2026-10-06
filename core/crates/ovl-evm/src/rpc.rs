//! Ethereum JSON-RPC reads and a local pending inbox.
//!
//! Reads come from the canonical execution world. `eth_sendRawTransaction`
//! validates a signed envelope and records it as pending. It does not commit
//! consensus state; inclusion is the version-2 block lane.

use std::collections::BTreeMap;

use agora_types::{OVL_EVM_PROFILE, OVL_EVM_REVM_VERSION};
use serde_json::{json, Value};

use crate::error::EvmError;
use crate::exec::{estimate_gas, eth_call};
use crate::tx::parse_raw_transaction;
use crate::world::OvlEvmWorld;

#[derive(Clone, Debug, Default)]
pub struct PendingTx {
    pub raw: Vec<u8>,
    pub from: [u8; 20],
    pub nonce: u64,
}

pub fn dispatch(
    world: &OvlEvmWorld,
    pending: &mut BTreeMap<[u8; 32], PendingTx>,
    method: &str,
    params: &Value,
) -> Result<Value, EvmError> {
    reject_foreign_asset(params)?;
    match method {
        "web3_clientVersion" => Ok(json!(format!(
            "agora-ovl-evm/{OVL_EVM_PROFILE}/revm-{OVL_EVM_REVM_VERSION}"
        ))),
        "net_version" => {
            world.require_active()?;
            Ok(json!(world.chain_id.to_string()))
        }
        "net_listening" => Ok(json!(false)),
        "net_peerCount" => Ok(json!("0x0")),
        "eth_chainId" => {
            world.require_active()?;
            Ok(json!(hex_qty(world.chain_id)))
        }
        "eth_syncing" => Ok(json!(false)),
        "eth_blockNumber" => {
            world.require_active()?;
            Ok(json!(hex_qty(world.block.number)))
        }
        "eth_gasPrice" => {
            world.require_active()?;
            Ok(json!(hex_qty(world.block.base_fee)))
        }
        "eth_maxPriorityFeePerGas" => {
            world.require_active()?;
            Ok(json!("0x1"))
        }
        "eth_getBalance" => {
            world.require_active()?;
            let address = address_param(params, 0)?;
            Ok(json!(hex_word(&world.balance(&address).to_be_bytes())))
        }
        "eth_getTransactionCount" => {
            world.require_active()?;
            let address = address_param(params, 0)?;
            Ok(json!(hex_qty(world.nonce(&address))))
        }
        "eth_getCode" => {
            world.require_active()?;
            let address = address_param(params, 0)?;
            Ok(json!(format!("0x{}", hex::encode(world.code(&address)))))
        }
        "eth_getStorageAt" => {
            world.require_active()?;
            let address = address_param(params, 0)?;
            let slot = word_param(params, 1)?;
            Ok(json!(hex_word(&world.storage_at(&address, &slot))))
        }
        "eth_call" => {
            let call = call_object(params)?;
            let out = eth_call(
                world,
                call.from,
                call.to,
                call.value,
                &call.data,
                call.gas.unwrap_or(world.block.gas_limit),
            )?;
            Ok(json!(format!("0x{}", hex::encode(out))))
        }
        "eth_estimateGas" => {
            let call = call_object(params)?;
            let gas = estimate_gas(world, call.from, call.to, call.value, &call.data)?;
            Ok(json!(hex_qty(gas)))
        }
        "eth_feeHistory" => fee_history(world, params),
        "eth_sendRawTransaction" => send_raw(world, pending, params),
        "eth_getTransactionByHash" => get_tx(world, pending, params),
        "eth_getTransactionReceipt" => get_receipt(world, params),
        "eth_getBlockByNumber" | "eth_getBlockByHash" => get_block(world, params),
        "eth_getLogs" => get_logs(world, params),
        "eth_getBlockTransactionCountByNumber" | "eth_getBlockTransactionCountByHash" => {
            world.require_active()?;
            Ok(json!(hex_qty(world.block.tx_hashes.len() as u64)))
        }
        "eth_getTransactionByBlockNumberAndIndex" | "eth_getTransactionByBlockHashAndIndex" => {
            get_tx_by_index(world, params)
        }
        other => Err(EvmError::rejected(format!("method not found: {other}"))),
    }
}

fn reject_foreign_asset(params: &Value) -> Result<(), EvmError> {
    let text = params.to_string().to_ascii_lowercase();
    if text.contains("\"drc\"") || text.contains("\"tlt\"") {
        return Err(EvmError::rejected("DRC and TLT cannot enter the OVL EVM"));
    }
    Ok(())
}

fn send_raw(
    world: &OvlEvmWorld,
    pending: &mut BTreeMap<[u8; 32], PendingTx>,
    params: &Value,
) -> Result<Value, EvmError> {
    world.require_active()?;
    let raw = hex_param(params, 0)?;
    let parsed = parse_raw_transaction(&raw)?;
    if parsed.chain_id != world.chain_id {
        return Err(EvmError::rejected("pending transaction chain id mismatch"));
    }
    let _ = parsed.intrinsic_gas()?;
    pending.insert(
        parsed.hash,
        PendingTx {
            raw,
            from: parsed.caller,
            nonce: parsed.nonce,
        },
    );
    Ok(json!(hex_word(&parsed.hash)))
}

fn get_tx(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    params: &Value,
) -> Result<Value, EvmError> {
    world.require_active()?;
    let hash = word_param(params, 0)?;
    if let Some(receipt) = world.receipts.iter().find(|receipt| receipt.hash == hash) {
        return Ok(json!({
            "hash": hex_word(&receipt.hash),
            "blockNumber": hex_qty(receipt.block_number),
            "blockHash": hex_word(&receipt.block_hash),
            "from": hex_addr(&receipt.from),
            "to": receipt.to.map(|addr| hex_addr(&addr)),
            "transactionIndex": hex_qty(receipt.index),
            "gas": hex_qty(receipt.gas_used),
        }));
    }
    if pending.contains_key(&hash) {
        return Ok(json!({
            "hash": hex_word(&hash),
            "blockNumber": Value::Null,
            "blockHash": Value::Null,
            "from": hex_addr(&pending[&hash].from),
        }));
    }
    Ok(Value::Null)
}

fn get_receipt(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let hash = word_param(params, 0)?;
    let Some(receipt) = world.receipts.iter().find(|receipt| receipt.hash == hash) else {
        return Ok(Value::Null);
    };
    Ok(receipt_json(receipt))
}

fn get_block(world: &OvlEvmWorld, _params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    Ok(json!({
        "number": hex_qty(world.block.number),
        "hash": hex_word(&world.block.hash),
        "parentHash": hex_word(&world.block.parent_hash),
        "timestamp": hex_qty(world.block.timestamp),
        "miner": hex_addr(&world.block.beneficiary),
        "gasLimit": hex_qty(world.block.gas_limit),
        "gasUsed": hex_qty(world.block.gas_used),
        "baseFeePerGas": hex_qty(world.block.base_fee),
        "transactions": world.block.tx_hashes.iter().map(hex_word).collect::<Vec<_>>(),
    }))
}

fn get_logs(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let filter_address = params
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("address"))
        .and_then(|value| value.as_str())
        .map(parse_address)
        .transpose()?;
    let mut logs = Vec::new();
    for receipt in &world.receipts {
        for (index, log) in receipt.logs.iter().enumerate() {
            if filter_address.is_some_and(|addr| addr != log.address) {
                continue;
            }
            logs.push(json!({
                "address": hex_addr(&log.address),
                "topics": log.topics.iter().map(hex_word).collect::<Vec<_>>(),
                "data": format!("0x{}", hex::encode(&log.data)),
                "blockNumber": hex_qty(receipt.block_number),
                "transactionHash": hex_word(&receipt.hash),
                "logIndex": hex_qty(index as u64),
            }));
        }
    }
    Ok(Value::Array(logs))
}

fn get_tx_by_index(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let index = qty_param(params, 1)? as usize;
    let Some(hash) = world.block.tx_hashes.get(index) else {
        return Ok(Value::Null);
    };
    get_tx(world, &BTreeMap::new(), &json!([hex_word(hash)]))
}

fn fee_history(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let count = qty_param(params, 0)?.clamp(1, 32);
    Ok(json!({
        "oldestBlock": hex_qty(world.block.number),
        "baseFeePerGas": (0..=count).map(|_| hex_qty(world.block.base_fee)).collect::<Vec<_>>(),
        "gasUsedRatio": (0..count).map(|_| {
            if world.block.gas_limit == 0 {
                0.0
            } else {
                world.block.gas_used as f64 / world.block.gas_limit as f64
            }
        }).collect::<Vec<_>>(),
        "reward": (0..count).map(|_| vec!["0x1".to_string()]).collect::<Vec<_>>(),
    }))
}

struct CallParams {
    from: [u8; 20],
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: Vec<u8>,
    gas: Option<u64>,
}

fn call_object(params: &Value) -> Result<CallParams, EvmError> {
    let object = params
        .as_array()
        .and_then(|items| items.first())
        .or_else(|| params.get("call"))
        .ok_or_else(|| EvmError::rejected("eth call object is missing"))?;
    let from = object
        .get("from")
        .and_then(|value| value.as_str())
        .map(parse_address)
        .transpose()?
        .unwrap_or([0u8; 20]);
    let to = object
        .get("to")
        .and_then(|value| value.as_str())
        .map(parse_address)
        .transpose()?;
    let value = object
        .get("value")
        .and_then(|value| value.as_str())
        .map(parse_word)
        .transpose()?
        .unwrap_or([0u8; 32]);
    let data = object
        .get("data")
        .and_then(|value| value.as_str())
        .map(parse_hex_bytes)
        .transpose()?
        .unwrap_or_default();
    let gas = object
        .get("gas")
        .and_then(|value| value.as_str())
        .map(parse_qty)
        .transpose()?;
    Ok(CallParams {
        from,
        to,
        value,
        data,
        gas,
    })
}

fn receipt_json(receipt: &crate::world::EvmReceipt) -> Value {
    json!({
        "transactionHash": hex_word(&receipt.hash),
        "transactionIndex": hex_qty(receipt.index),
        "blockHash": hex_word(&receipt.block_hash),
        "blockNumber": hex_qty(receipt.block_number),
        "from": hex_addr(&receipt.from),
        "to": receipt.to.map(|addr| hex_addr(&addr)),
        "contractAddress": receipt.contract_address.map(|addr| hex_addr(&addr)),
        "gasUsed": hex_qty(receipt.gas_used),
        "cumulativeGasUsed": hex_qty(receipt.cumulative_gas_used),
        "effectiveGasPrice": hex_qty_u128(receipt.effective_gas_price),
        "status": if receipt.status { "0x1" } else { "0x0" },
        "logs": receipt.logs.iter().enumerate().map(|(index, log)| json!({
            "address": hex_addr(&log.address),
            "topics": log.topics.iter().map(hex_word).collect::<Vec<_>>(),
            "data": format!("0x{}", hex::encode(&log.data)),
            "logIndex": hex_qty(index as u64),
            "transactionHash": hex_word(&receipt.hash),
            "blockNumber": hex_qty(receipt.block_number),
        })).collect::<Vec<_>>(),
    })
}

fn address_param(params: &Value, index: usize) -> Result<[u8; 20], EvmError> {
    let text = string_param(params, index)?;
    parse_address(&text)
}

fn word_param(params: &Value, index: usize) -> Result<[u8; 32], EvmError> {
    parse_word(&string_param(params, index)?)
}

fn qty_param(params: &Value, index: usize) -> Result<u64, EvmError> {
    let value = param(params, index)?;
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    parse_qty(
        value
            .as_str()
            .ok_or_else(|| EvmError::rejected("quantity param"))?,
    )
}

fn hex_param(params: &Value, index: usize) -> Result<Vec<u8>, EvmError> {
    parse_hex_bytes(&string_param(params, index)?)
}

fn string_param(params: &Value, index: usize) -> Result<String, EvmError> {
    param(params, index)?
        .as_str()
        .map(|value| value.to_string())
        .ok_or_else(|| EvmError::rejected("string param"))
}

fn param(params: &Value, index: usize) -> Result<&Value, EvmError> {
    if let Some(items) = params.as_array() {
        return items
            .get(index)
            .ok_or_else(|| EvmError::rejected("missing param"));
    }
    Err(EvmError::rejected("expected positional params"))
}

fn parse_address(text: &str) -> Result<[u8; 20], EvmError> {
    let bytes = parse_hex_bytes(text)?;
    if bytes.len() != 20 {
        return Err(EvmError::rejected("address must be 20 bytes"));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_word(text: &str) -> Result<[u8; 32], EvmError> {
    let bytes = parse_hex_bytes(text)?;
    if bytes.len() > 32 {
        return Err(EvmError::rejected("word exceeds 32 bytes"));
    }
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(out)
}

fn parse_qty(text: &str) -> Result<u64, EvmError> {
    let text = text.strip_prefix("0x").unwrap_or(text);
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 16).map_err(|_| EvmError::rejected("bad hex quantity"))
}

fn parse_hex_bytes(text: &str) -> Result<Vec<u8>, EvmError> {
    let text = text.strip_prefix("0x").unwrap_or(text);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    hex::decode(text).map_err(|_| EvmError::rejected("bad hex bytes"))
}

fn hex_qty(value: u64) -> String {
    format!("0x{value:x}")
}

fn hex_qty_u128(value: u128) -> String {
    format!("0x{value:x}")
}

fn hex_word(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn hex_addr(bytes: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(bytes))
}
