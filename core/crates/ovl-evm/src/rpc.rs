//! Ethereum JSON-RPC reads and a process-local pending inbox.
//!
//! Reads come from the canonical execution world. `eth_sendRawTransaction`
//! validates a signed envelope and records it as pending. It does not commit
//! consensus state and it does not publish on the Agora-signed mempool.
//! Cross-node gossip of raw envelopes is PLANNED: version 2 is rejected by
//! that mempool, and the existing fingerprint has no separate raw-EVM topic.

use std::collections::BTreeMap;

use agora_types::{OVL_EVM_PROFILE, OVL_EVM_REVM_VERSION};
use serde_json::{json, Value};

use crate::error::EvmError;
use crate::exec::{estimate_gas, eth_call};
use crate::fee::next_base_fee;
use crate::tx::{parse_raw_transaction, ParsedTx};
use crate::world::{EvmReceipt, OvlEvmWorld};

#[derive(Clone, Debug, Default)]
pub struct PendingTx {
    pub raw: Vec<u8>,
    pub from: [u8; 20],
    pub nonce: u64,
}

/// Sync progress this process can actually report.
///
/// `None` becomes JSON `false`. A boolean `true` is not used, because the
/// Ethereum object would otherwise be filled with invented block numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EthSyncStatus {
    pub starting_block: u64,
    pub current_block: u64,
    pub highest_block: u64,
}

/// Node facts that are not fields of the execution world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EthNodeView {
    pub listening: bool,
    pub peer_count: u64,
    pub syncing: Option<EthSyncStatus>,
    /// `eth_sendRawTransaction` stays closed unless the caller is a dev or
    /// test process. The execution world must also be active.
    pub accept_raw_transactions: bool,
}

impl EthNodeView {
    /// Engine unit tests submit into the process-local inbox.
    fn engine_tests() -> Self {
        Self {
            accept_raw_transactions: true,
            ..Self::default()
        }
    }
}

pub fn dispatch(
    world: &OvlEvmWorld,
    pending: &mut BTreeMap<[u8; 32], PendingTx>,
    method: &str,
    params: &Value,
) -> Result<Value, EvmError> {
    dispatch_with_view(world, pending, &EthNodeView::engine_tests(), method, params)
}

pub fn dispatch_with_view(
    world: &OvlEvmWorld,
    pending: &mut BTreeMap<[u8; 32], PendingTx>,
    view: &EthNodeView,
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
        "net_listening" => Ok(json!(view.listening)),
        "net_peerCount" => Ok(json!(hex_qty(view.peer_count))),
        "eth_chainId" => {
            world.require_active()?;
            Ok(json!(hex_qty(world.chain_id)))
        }
        "eth_syncing" => Ok(syncing_json(view)),
        "eth_blockNumber" => {
            world.require_active()?;
            Ok(json!(hex_qty(world.block.number)))
        }
        "eth_gasPrice" => {
            world.require_active()?;
            let price = u128::from(world.block.base_fee).saturating_add(suggested_tip(world));
            Ok(json!(hex_qty_u128(price)))
        }
        "eth_maxPriorityFeePerGas" => {
            world.require_active()?;
            Ok(json!(hex_qty_u128(suggested_tip(world))))
        }
        "eth_getBalance" => {
            require_committed_state(world, params, 1)?;
            let address = address_param(params, 0)?;
            Ok(json!(hex_word(&world.balance(&address).to_be_bytes())))
        }
        "eth_getTransactionCount" => {
            world.require_active()?;
            let address = address_param(params, 0)?;
            let pending_tag = matches!(optional_block_ref(params, 1)?, Some(BlockRef::Pending));
            if !pending_tag {
                require_committed_state(world, params, 1)?;
            }
            let nonce = if pending_tag {
                pending_nonce(world, pending, &address)
            } else {
                world.nonce(&address)
            };
            Ok(json!(hex_qty(nonce)))
        }
        "eth_getCode" => {
            require_committed_state(world, params, 1)?;
            let address = address_param(params, 0)?;
            Ok(json!(format!("0x{}", hex::encode(world.code(&address)))))
        }
        "eth_getStorageAt" => {
            require_committed_state(world, params, 2)?;
            let address = address_param(params, 0)?;
            let slot = word_param(params, 1)?;
            Ok(json!(hex_word(&world.storage_at(&address, &slot))))
        }
        "eth_call" => {
            require_committed_state(world, params, 1)?;
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
            require_committed_state(world, params, 1)?;
            let call = call_object(params)?;
            let gas = estimate_gas(world, call.from, call.to, call.value, &call.data)?;
            Ok(json!(hex_qty(gas)))
        }
        "eth_feeHistory" => fee_history(world, params),
        "eth_sendRawTransaction" => send_raw(world, pending, view, params),
        "eth_getTransactionByHash" => get_tx(world, pending, params),
        "eth_getTransactionReceipt" => get_receipt(world, params),
        "eth_getBlockByNumber" | "eth_getBlockByHash" => get_block(world, pending, method, params),
        "eth_getLogs" => get_logs(world, params),
        "eth_getBlockTransactionCountByNumber" | "eth_getBlockTransactionCountByHash" => {
            block_tx_count(world, pending, method, params)
        }
        "eth_getTransactionByBlockNumberAndIndex" | "eth_getTransactionByBlockHashAndIndex" => {
            get_tx_by_index(world, pending, method, params)
        }
        "eth_getProof" => Err(EvmError::MethodNotFound(method.to_string())),
        other => Err(EvmError::MethodNotFound(other.to_string())),
    }
}

fn syncing_json(view: &EthNodeView) -> Value {
    match &view.syncing {
        None => Value::Bool(false),
        Some(status) => json!({
            "startingBlock": hex_qty(status.starting_block),
            "currentBlock": hex_qty(status.current_block),
            "highestBlock": hex_qty(status.highest_block),
        }),
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
    view: &EthNodeView,
    params: &Value,
) -> Result<Value, EvmError> {
    if !view.accept_raw_transactions {
        return Err(EvmError::rejected(
            "eth_sendRawTransaction is limited to dev and test activation",
        ));
    }
    world.require_active()?;
    let raw = hex_param(params, 0)?;
    let parsed = parse_raw_transaction(&raw)?;
    if parsed.chain_id != world.chain_id {
        return Err(EvmError::rejected("pending transaction chain id mismatch"));
    }
    let _ = parsed.intrinsic_gas()?;
    if world
        .receipts
        .iter()
        .any(|receipt| receipt.hash == parsed.hash)
    {
        return Ok(json!(hex_word(&parsed.hash)));
    }
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
        return transaction_json(&parse_raw_transaction(&receipt.raw)?, Some(receipt));
    }
    if let Some(tx) = pending.get(&hash) {
        return transaction_json(&parse_raw_transaction(&tx.raw)?, None);
    }
    Ok(Value::Null)
}

fn get_receipt(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let hash = word_param(params, 0)?;
    let Some(receipt) = world.receipts.iter().find(|receipt| receipt.hash == hash) else {
        return Ok(Value::Null);
    };
    receipt_json(world, receipt)
}

fn get_block(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    method: &str,
    params: &Value,
) -> Result<Value, EvmError> {
    world.require_active()?;
    let Some(query) = resolve_block(world, method, params)? else {
        return Ok(Value::Null);
    };
    block_json(world, pending, &query)
}

fn block_tx_count(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    method: &str,
    params: &Value,
) -> Result<Value, EvmError> {
    world.require_active()?;
    let Some(query) = resolve_block(world, method, params)? else {
        return Ok(Value::Null);
    };
    Ok(json!(hex_qty(
        block_transactions(world, pending, query.pending).len() as u64
    )))
}

fn get_tx_by_index(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    method: &str,
    params: &Value,
) -> Result<Value, EvmError> {
    world.require_active()?;
    let Some(query) = resolve_block(world, method, params)? else {
        return Ok(Value::Null);
    };
    let index = qty_param(params, 1)? as usize;
    let Some(hash) = block_transactions(world, pending, query.pending)
        .get(index)
        .copied()
    else {
        return Ok(Value::Null);
    };
    get_tx(world, pending, &json!([hex_word(&hash)]))
}

fn get_logs(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let filter = params
        .as_array()
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let filter_address = filter
        .get("address")
        .map(parse_address_filter)
        .transpose()?;
    let from_block = filter.get("fromBlock").map(parse_block_ref).transpose()?;
    let to_block = filter.get("toBlock").map(parse_block_ref).transpose()?;
    let block_hash = filter
        .get("blockHash")
        .and_then(|value| value.as_str())
        .map(parse_word)
        .transpose()?;
    let mut logs = Vec::new();
    for receipt in &world.receipts {
        if !log_block_matches(
            world,
            receipt,
            from_block.as_ref(),
            to_block.as_ref(),
            block_hash,
        ) {
            continue;
        }
        for (index, log) in receipt.logs.iter().enumerate() {
            if filter_address
                .as_ref()
                .is_some_and(|addresses| !addresses.contains(&log.address))
            {
                continue;
            }
            if !topics_match(&log.topics, filter.get("topics")) {
                continue;
            }
            logs.push(log_json(world, receipt, index, log));
        }
    }
    Ok(Value::Array(logs))
}

fn fee_history(world: &OvlEvmWorld, params: &Value) -> Result<Value, EvmError> {
    world.require_active()?;
    let requested = qty_param(params, 0)?;
    if requested == 0 || requested > 1024 {
        return Err(EvmError::rejected(
            "fee history block count must be between 1 and 1024",
        ));
    }
    match optional_block_ref(params, 1)? {
        None | Some(BlockRef::Latest) | Some(BlockRef::Pending) => {}
        Some(BlockRef::Number(number)) if number == world.block.number => {}
        Some(BlockRef::Earliest) if world.block.number == 0 => {}
        Some(_) => {
            return Err(EvmError::rejected(
                "historical OVL fee data is not retained",
            ))
        }
    }
    // Only the open canonical block is stored. Returning that one block is the
    // available range; earlier slots are not copies of the head.
    let next = next_base_fee(world.block.base_fee, world.block.gas_used, &world.fee)?;
    let percentiles = percentile_param(params, 2)?;
    let mut body = json!({
        "oldestBlock": hex_qty(world.block.number),
        "baseFeePerGas": [hex_qty(world.block.base_fee), hex_qty(next)],
        "gasUsedRatio": [gas_used_ratio(world)],
    });
    if !percentiles.is_empty() {
        body["reward"] = json!([reward_percentiles(world, &percentiles)]);
    }
    Ok(body)
}

fn suggested_tip(world: &OvlEvmWorld) -> u128 {
    let base = u128::from(world.block.base_fee);
    world
        .receipts
        .iter()
        .filter(|receipt| receipt.block_number == world.block.number)
        .map(|receipt| receipt.effective_gas_price.saturating_sub(base))
        .min()
        .unwrap_or(1)
}

fn reward_percentiles(world: &OvlEvmWorld, percentiles: &[f64]) -> Vec<String> {
    let base = u128::from(world.block.base_fee);
    let mut tips: Vec<u128> = world
        .receipts
        .iter()
        .filter(|receipt| receipt.block_number == world.block.number)
        .map(|receipt| receipt.effective_gas_price.saturating_sub(base))
        .collect();
    tips.sort_unstable();
    if tips.is_empty() {
        return percentiles.iter().map(|_| "0x0".to_string()).collect();
    }
    percentiles
        .iter()
        .map(|percentile| hex_qty_u128(tips[percentile_index(tips.len(), *percentile)]))
        .collect()
}

fn percentile_index(len: usize, percentile: f64) -> usize {
    if len <= 1 {
        return 0;
    }
    let bounded = percentile.clamp(0.0, 100.0);
    let rank = ((len - 1) as f64) * (bounded / 100.0);
    if !rank.is_finite() {
        return 0;
    }
    let rank = rank.round();
    if rank < 0.0 {
        0
    } else {
        (rank as usize).min(len - 1)
    }
}

fn gas_used_ratio(world: &OvlEvmWorld) -> f64 {
    if world.block.gas_limit == 0 {
        0.0
    } else {
        world.block.gas_used as f64 / world.block.gas_limit as f64
    }
}

fn pending_nonce(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    address: &[u8; 20],
) -> u64 {
    let mut next = world.nonce(address);
    let mut queued: Vec<u64> = pending
        .values()
        .filter(|tx| tx.from == *address && tx.nonce >= next)
        .map(|tx| tx.nonce)
        .collect();
    queued.sort_unstable();
    queued.dedup();
    for nonce in queued {
        if nonce == next {
            next = next.saturating_add(1);
        } else {
            break;
        }
    }
    next
}

#[derive(Clone, Copy)]
struct BlockQuery {
    pending: bool,
    full: bool,
}

fn resolve_block(
    world: &OvlEvmWorld,
    method: &str,
    params: &Value,
) -> Result<Option<BlockQuery>, EvmError> {
    let full = params
        .as_array()
        .and_then(|items| items.get(1))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let tag = if method.contains("ByHash") {
        BlockRef::Hash(word_param(params, 0)?)
    } else {
        parse_block_ref(param(params, 0)?)?
    };
    let matches_head = match tag {
        BlockRef::Latest => true,
        BlockRef::Pending => true,
        BlockRef::Earliest => world.block.number == 0,
        BlockRef::Number(number) => number == world.block.number,
        BlockRef::Hash(hash) => hash == world.block.hash,
        BlockRef::Unavailable(_) => false,
    };
    if !matches_head {
        return Ok(None);
    }
    Ok(Some(BlockQuery {
        pending: matches!(tag, BlockRef::Pending),
        full,
    }))
}

fn block_transactions(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    include_pending: bool,
) -> Vec<[u8; 32]> {
    let mut hashes = world.block.tx_hashes.clone();
    if include_pending {
        for hash in pending.keys() {
            if !hashes.contains(hash) {
                hashes.push(*hash);
            }
        }
    }
    hashes
}

fn block_json(
    world: &OvlEvmWorld,
    pending: &BTreeMap<[u8; 32], PendingTx>,
    query: &BlockQuery,
) -> Result<Value, EvmError> {
    let hashes = block_transactions(world, pending, query.pending);
    let transactions = if query.full {
        let mut objects = Vec::with_capacity(hashes.len());
        for hash in &hashes {
            objects.push(get_tx(world, pending, &json!([hex_word(hash)]))?);
        }
        Value::Array(objects)
    } else {
        json!(hashes.iter().map(hex_word).collect::<Vec<_>>())
    };
    let hash = if query.pending {
        Value::Null
    } else {
        json!(hex_word(&world.block.hash))
    };
    Ok(json!({
        "number": hex_qty(world.block.number),
        "hash": hash,
        "parentHash": hex_word(&world.block.parent_hash),
        "timestamp": hex_qty(world.block.timestamp),
        "miner": hex_addr(&world.block.beneficiary),
        "gasLimit": hex_qty(world.block.gas_limit),
        "gasUsed": hex_qty(world.block.gas_used),
        "baseFeePerGas": hex_qty(world.block.base_fee),
        "transactions": transactions,
    }))
}

fn transaction_json(parsed: &ParsedTx, mined: Option<&EvmReceipt>) -> Result<Value, EvmError> {
    let gas_price = if parsed.tx_type == 2 {
        mined
            .map(|receipt| receipt.effective_gas_price)
            .unwrap_or(parsed.max_fee_per_gas)
    } else {
        parsed.max_fee_per_gas
    };
    let mut body = json!({
        "hash": hex_word(&parsed.hash),
        "nonce": hex_qty(parsed.nonce),
        "blockHash": mined.map(|receipt| hex_word(&receipt.block_hash)),
        "blockNumber": mined.map(|receipt| hex_qty(receipt.block_number)),
        "transactionIndex": mined.map(|receipt| hex_qty(receipt.index)),
        "from": hex_addr(&parsed.caller),
        "to": parsed.to.map(|addr| hex_addr(&addr)),
        "value": hex_word(&parsed.value),
        "gas": hex_qty(parsed.gas_limit),
        "gasPrice": hex_qty_u128(gas_price),
        "input": format!("0x{}", hex::encode(&parsed.data)),
        "type": hex_qty(u64::from(parsed.tx_type)),
        "chainId": hex_qty(parsed.chain_id),
    });
    if parsed.tx_type == 2 {
        body["maxFeePerGas"] = json!(hex_qty_u128(parsed.max_fee_per_gas));
        body["maxPriorityFeePerGas"] =
            json!(hex_qty_u128(parsed.max_priority_fee_per_gas.unwrap_or(0)));
    }
    if parsed.tx_type == 1 || parsed.tx_type == 2 {
        body["accessList"] = json!(parsed
            .access_list
            .iter()
            .map(|item| json!({
                "address": hex_addr(&item.address),
                "storageKeys": item.storage_keys.iter().map(hex_word).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>());
    }
    Ok(body)
}

fn receipt_json(world: &OvlEvmWorld, receipt: &EvmReceipt) -> Result<Value, EvmError> {
    let tx_type = parse_raw_transaction(&receipt.raw)
        .ok()
        .map(|parsed| parsed.tx_type);
    let mut body = json!({
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
        "logs": receipt.logs.iter().enumerate().map(|(index, log)| {
            log_json(world, receipt, index, log)
        }).collect::<Vec<_>>(),
    });
    if let Some(tx_type) = tx_type {
        body["type"] = json!(hex_qty(u64::from(tx_type)));
    }
    Ok(body)
}

fn log_json(
    world: &OvlEvmWorld,
    receipt: &EvmReceipt,
    index: usize,
    log: &crate::world::EvmLog,
) -> Value {
    json!({
        "address": hex_addr(&log.address),
        "topics": log.topics.iter().map(hex_word).collect::<Vec<_>>(),
        "data": format!("0x{}", hex::encode(&log.data)),
        "blockNumber": hex_qty(receipt.block_number),
        "blockHash": hex_word(&receipt.block_hash),
        "transactionHash": hex_word(&receipt.hash),
        "transactionIndex": hex_qty(receipt.index),
        "logIndex": hex_qty(block_log_index(world, receipt, index)),
        "removed": false,
    })
}

fn block_log_index(world: &OvlEvmWorld, receipt: &EvmReceipt, local: usize) -> u64 {
    let prior: u64 = world
        .receipts
        .iter()
        .filter(|other| other.block_hash == receipt.block_hash && other.index < receipt.index)
        .map(|other| other.logs.len() as u64)
        .sum();
    prior + local as u64
}

fn log_block_matches(
    world: &OvlEvmWorld,
    receipt: &EvmReceipt,
    from_block: Option<&BlockRef>,
    to_block: Option<&BlockRef>,
    block_hash: Option<[u8; 32]>,
) -> bool {
    if let Some(hash) = block_hash {
        if receipt.block_hash != hash {
            return false;
        }
    }
    if let Some(bound) = from_block.and_then(|tag| block_bound(world, tag)) {
        if receipt.block_number < bound {
            return false;
        }
    }
    if let Some(bound) = to_block.and_then(|tag| block_bound(world, tag)) {
        if receipt.block_number > bound {
            return false;
        }
    }
    true
}

fn block_bound(world: &OvlEvmWorld, tag: &BlockRef) -> Option<u64> {
    match tag {
        BlockRef::Latest | BlockRef::Pending => Some(world.block.number),
        BlockRef::Earliest => Some(0),
        BlockRef::Number(number) => Some(*number),
        BlockRef::Hash(hash) if *hash == world.block.hash => Some(world.block.number),
        BlockRef::Hash(_) | BlockRef::Unavailable(_) => None,
    }
}

fn topics_match(actual: &[[u8; 32]], filter: Option<&Value>) -> bool {
    let Some(Value::Array(items)) = filter else {
        return true;
    };
    for (index, wanted) in items.iter().enumerate() {
        if wanted.is_null() {
            continue;
        }
        let Some(topic) = actual.get(index) else {
            return false;
        };
        if let Some(text) = wanted.as_str() {
            if parse_word(text).ok().as_ref() != Some(topic) {
                return false;
            }
        } else if let Some(options) = wanted.as_array() {
            let matched = options.iter().any(|option| {
                option
                    .as_str()
                    .and_then(|text| parse_word(text).ok())
                    .as_ref()
                    == Some(topic)
            });
            if !matched {
                return false;
            }
        }
    }
    true
}

fn parse_address_filter(value: &Value) -> Result<Vec<[u8; 20]>, EvmError> {
    if let Some(text) = value.as_str() {
        return Ok(vec![parse_address(text)?]);
    }
    let Some(items) = value.as_array() else {
        return Err(EvmError::rejected("log address filter"));
    };
    items
        .iter()
        .map(|item| {
            parse_address(
                item.as_str()
                    .ok_or_else(|| EvmError::rejected("log address filter"))?,
            )
        })
        .collect()
}

#[derive(Clone, Debug)]
enum BlockRef {
    Latest,
    Pending,
    Earliest,
    Number(u64),
    Hash([u8; 32]),
    Unavailable(String),
}

fn require_committed_state(
    world: &OvlEvmWorld,
    params: &Value,
    index: usize,
) -> Result<(), EvmError> {
    world.require_active()?;
    match optional_block_ref(params, index)? {
        None | Some(BlockRef::Latest) => Ok(()),
        Some(BlockRef::Number(number)) if number == world.block.number => Ok(()),
        Some(BlockRef::Hash(hash)) if hash == world.block.hash => Ok(()),
        Some(BlockRef::Earliest) if world.block.number == 0 => Ok(()),
        Some(BlockRef::Pending) => Err(EvmError::rejected(
            "pending OVL state is not simulated; the inbox does not overlay canonical balances",
        )),
        Some(BlockRef::Unavailable(name)) => Err(EvmError::rejected(format!(
            "block tag {name} is not available for OVL execution state"
        ))),
        Some(_) => Err(EvmError::rejected(
            "historical OVL account state is not retained",
        )),
    }
}

fn optional_block_ref(params: &Value, index: usize) -> Result<Option<BlockRef>, EvmError> {
    let Some(value) = params.as_array().and_then(|items| items.get(index)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(parse_block_ref(value)?))
}

fn parse_block_ref(value: &Value) -> Result<BlockRef, EvmError> {
    if let Some(number) = value.as_u64() {
        return Ok(BlockRef::Number(number));
    }
    let text = value
        .as_str()
        .ok_or_else(|| EvmError::rejected("block tag"))?;
    match text {
        "latest" => Ok(BlockRef::Latest),
        "pending" => Ok(BlockRef::Pending),
        "earliest" => Ok(BlockRef::Earliest),
        "safe" | "finalized" => Ok(BlockRef::Unavailable(text.to_string())),
        _ => {
            // Quantity tags such as `0x0` are odd-length. Only a 32-byte
            // digest is a block hash; everything else is a block number.
            let body = text.strip_prefix("0x").unwrap_or(text);
            if body.len() == 64 {
                let bytes = parse_hex_bytes(text)?;
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&bytes);
                return Ok(BlockRef::Hash(hash));
            }
            Ok(BlockRef::Number(parse_qty(text)?))
        }
    }
}

fn percentile_param(params: &Value, index: usize) -> Result<Vec<f64>, EvmError> {
    let Some(value) = params.as_array().and_then(|items| items.get(index)) else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let Some(items) = value.as_array() else {
        return Err(EvmError::rejected("fee history percentiles"));
    };
    items
        .iter()
        .map(|item| {
            item.as_f64()
                .ok_or_else(|| EvmError::rejected("fee history percentile"))
        })
        .collect()
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
        .or_else(|| object.get("input"))
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
    // Storage slots and values are quantities, so `0x0` is a zero word.
    let body = text.strip_prefix("0x").unwrap_or(text);
    let padded;
    let even = if body.len() % 2 == 1 {
        padded = format!("0{body}");
        padded.as_str()
    } else {
        body
    };
    let bytes = if even.is_empty() {
        Vec::new()
    } else {
        hex::decode(even).map_err(|_| EvmError::rejected("bad hex bytes"))?
    };
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
