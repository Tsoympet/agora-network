//! Pinned `revm` 42.0.1 adapter at `SpecId::SHANGHAI`.
//!
//! `Context::mainnet()` is intentionally unused. The historical rollup's fixed
//! caller, unsigned compact transactions, and unknown-root reseeding are not
//! accepted by this adapter.

use agora_types::OvlWei;
use alloy_primitives::{Address, Bytes, B256, U256};
use revm::context::result::ExecutionResult;
use revm::context::transaction::{AccessList, AccessListItem};
use revm::context::{BlockEnv, CfgEnv, TxEnv};
use revm::database::{CacheDB, EmptyDB};
use revm::primitives::hardfork::SpecId;
use revm::primitives::{TxKind, KECCAK_EMPTY};
use revm::state::{AccountInfo, Bytecode};
use revm::{Context, ExecuteCommitEvm, ExecuteEvm, MainBuilder};

use crate::error::EvmError;
use crate::tx::{parse_raw_transaction, ParsedTx};
use crate::world::{EvmAccount, EvmLog, EvmReceipt, OvlEvmWorld};

pub fn apply_raw_transaction(world: &mut OvlEvmWorld, raw: &[u8]) -> Result<EvmReceipt, EvmError> {
    world.require_active()?;
    let parsed = parse_raw_transaction(raw)?;
    validate_against_world(world, &parsed)?;
    let base_fee = u128::from(world.block.base_fee);
    let effective = parsed.effective_price(base_fee);
    let db = cache_from_world(world);
    let tx = tx_env(&parsed)?;
    let (db, result) = run_commit(world, db, tx)?;
    overlay_cache(world, &db);

    let gas_used = result.tx_gas_used();
    let burn = OvlWei::from_u64(world.block.base_fee)
        .checked_mul_u64(gas_used)
        .ok_or_else(|| EvmError::rejected("base fee burn overflow"))?;
    world.burned = world
        .burned
        .checked_add(burn)
        .ok_or_else(|| EvmError::rejected("burned supply overflow"))?;
    world.block.gas_used = world
        .block
        .gas_used
        .checked_add(gas_used)
        .ok_or_else(|| EvmError::rejected("block gas overflow"))?;
    world.block.tx_hashes.push(parsed.hash);
    let receipt = EvmReceipt {
        hash: parsed.hash,
        index: world.receipts.len() as u64,
        block_number: world.block.number,
        block_hash: world.block.hash,
        from: parsed.caller,
        to: parsed.to,
        contract_address: result.created_address().map(|addr| addr.into_array()),
        gas_used,
        cumulative_gas_used: world.block.gas_used,
        status: result.is_success(),
        effective_gas_price: effective,
        logs: result
            .logs()
            .iter()
            .map(|log| EvmLog {
                address: log.address.into_array(),
                topics: log
                    .data
                    .topics()
                    .iter()
                    .copied()
                    .map(|topic| topic.0)
                    .collect(),
                data: log.data.data.to_vec(),
            })
            .collect(),
        output: result
            .output()
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default(),
        raw: raw.to_vec(),
    };
    world.receipts.push(receipt.clone());
    world.check_supply()?;
    Ok(receipt)
}

/// Gas reported by a second, non-committing Shanghai execution of `raw`.
pub fn measure_shanghai_gas(world: &OvlEvmWorld, raw: &[u8]) -> Result<u64, EvmError> {
    world.require_active()?;
    let parsed = parse_raw_transaction(raw)?;
    validate_against_world(world, &parsed)?;
    let db = cache_from_world(world);
    let tx = tx_env(&parsed)?;
    let result = run_call(world, db, tx)?;
    Ok(result.tx_gas_used())
}

pub fn eth_call(
    world: &OvlEvmWorld,
    from: [u8; 20],
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
    gas_limit: u64,
) -> Result<Vec<u8>, EvmError> {
    world.require_active()?;
    let parsed = ParsedTx {
        tx_type: 2,
        chain_id: world.chain_id,
        nonce: world.nonce(&from),
        gas_limit,
        max_fee_per_gas: u128::from(world.block.base_fee),
        max_priority_fee_per_gas: Some(0),
        to,
        value,
        data: data.to_vec(),
        access_list: Vec::new(),
        caller: from,
        hash: [0u8; 32],
    };
    let db = cache_from_world(world);
    let tx = tx_env(&parsed)?;
    let result = run_call(world, db, tx)?;
    if !result.is_success() {
        let detail = match &result {
            ExecutionResult::Revert { output, .. } => format!("revert 0x{}", hex::encode(output)),
            ExecutionResult::Halt { reason, .. } => format!("halt {reason:?}"),
            ExecutionResult::Success { .. } => "success".to_string(),
        };
        return Err(EvmError::rejected(format!("eth_call failed: {detail}")));
    }
    Ok(result
        .output()
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default())
}

pub fn estimate_gas(
    world: &OvlEvmWorld,
    from: [u8; 20],
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
) -> Result<u64, EvmError> {
    let stub = ParsedTx {
        tx_type: 2,
        chain_id: world.chain_id,
        nonce: 0,
        gas_limit: 0,
        max_fee_per_gas: 0,
        max_priority_fee_per_gas: None,
        to,
        value,
        data: data.to_vec(),
        access_list: Vec::new(),
        caller: from,
        hash: [0u8; 32],
    };
    let mut lo = stub.intrinsic_gas()?;
    let mut hi = world.block.gas_limit;
    if eth_call(world, from, to, value, data, hi).is_err() {
        return Err(EvmError::rejected(
            "transaction cannot execute within the block gas limit",
        ));
    }
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if eth_call(world, from, to, value, data, mid).is_ok() {
            hi = mid;
        } else {
            lo = mid.saturating_add(1);
        }
    }
    Ok(lo)
}

fn validate_against_world(world: &OvlEvmWorld, parsed: &ParsedTx) -> Result<(), EvmError> {
    if parsed.chain_id != world.chain_id {
        return Err(EvmError::rejected(format!(
            "chain id {} does not match OVL chain {}",
            parsed.chain_id, world.chain_id
        )));
    }
    let intrinsic = parsed.intrinsic_gas()?;
    if parsed.gas_limit < intrinsic {
        return Err(EvmError::rejected(format!(
            "gas limit {} is below intrinsic {}",
            parsed.gas_limit, intrinsic
        )));
    }
    let remaining = world.block.gas_limit.saturating_sub(world.block.gas_used);
    if parsed.gas_limit > remaining {
        return Err(EvmError::rejected("gas limit exceeds remaining block gas"));
    }
    if world.nonce(&parsed.caller) != parsed.nonce {
        return Err(EvmError::rejected(format!(
            "bad OVL execution nonce: got {} expected {}",
            parsed.nonce,
            world.nonce(&parsed.caller)
        )));
    }
    let base_fee = u128::from(world.block.base_fee);
    if parsed.max_fee_per_gas < base_fee || parsed.effective_price(base_fee) < base_fee {
        return Err(EvmError::rejected("effective price is below base fee"));
    }
    Ok(())
}

fn run_commit(
    world: &OvlEvmWorld,
    db: CacheDB<EmptyDB>,
    tx: TxEnv,
) -> Result<(CacheDB<EmptyDB>, ExecutionResult), EvmError> {
    let mut evm = shanghai_context(world, db).build_mainnet();
    let result = evm
        .transact_commit(tx)
        .map_err(|err| EvmError::rejected(format!("revm rejected transaction: {err:?}")))?;
    Ok((evm.ctx.journaled_state.database, result))
}

fn run_call(
    world: &OvlEvmWorld,
    db: CacheDB<EmptyDB>,
    tx: TxEnv,
) -> Result<ExecutionResult, EvmError> {
    let mut evm = shanghai_context(world, db).build_mainnet();
    let result = evm
        .transact(tx)
        .map_err(|err| EvmError::rejected(format!("eth_call failed: {err:?}")))?;
    Ok(result.result)
}

fn shanghai_context(
    world: &OvlEvmWorld,
    db: CacheDB<EmptyDB>,
) -> Context<BlockEnv, TxEnv, CfgEnv, CacheDB<EmptyDB>> {
    let mut ctx: Context<BlockEnv, TxEnv, CfgEnv, CacheDB<EmptyDB>> =
        Context::new(db, SpecId::SHANGHAI);
    ctx.cfg.chain_id = world.chain_id;
    ctx.block.number = U256::from(world.block.number);
    ctx.block.beneficiary = Address::new(world.block.beneficiary);
    ctx.block.timestamp = U256::from(world.block.timestamp);
    ctx.block.gas_limit = world.block.gas_limit;
    ctx.block.basefee = world.block.base_fee;
    ctx.block.difficulty = U256::ZERO;
    ctx.block.prevrandao = Some(B256::from(world.block.hash));
    ctx.block.blob_excess_gas_and_price = None;
    ctx
}

fn tx_env(parsed: &ParsedTx) -> Result<TxEnv, EvmError> {
    let kind = match parsed.to {
        Some(addr) => TxKind::Call(Address::new(addr)),
        None => TxKind::Create,
    };
    let access = AccessList(
        parsed
            .access_list
            .iter()
            .map(|item| AccessListItem {
                address: Address::new(item.address),
                storage_keys: item.storage_keys.iter().copied().map(B256::from).collect(),
            })
            .collect(),
    );
    TxEnv::builder()
        .tx_type(Some(parsed.tx_type))
        .caller(Address::new(parsed.caller))
        .gas_limit(parsed.gas_limit)
        .gas_price(parsed.max_fee_per_gas)
        .gas_priority_fee(parsed.max_priority_fee_per_gas)
        .kind(kind)
        .value(U256::from_be_slice(&parsed.value))
        .data(Bytes::copy_from_slice(&parsed.data))
        .nonce(parsed.nonce)
        .chain_id(Some(parsed.chain_id))
        .access_list(access)
        .build()
        .map_err(|err| EvmError::rejected(format!("tx env: {err:?}")))
}

fn cache_from_world(world: &OvlEvmWorld) -> CacheDB<EmptyDB> {
    let mut db = CacheDB::new(EmptyDB::default());
    for (addr, account) in &world.accounts {
        let code = if account.code.is_empty() {
            Bytecode::default()
        } else {
            Bytecode::new_raw(Bytes::copy_from_slice(&account.code))
        };
        db.insert_account_info(
            Address::new(*addr),
            AccountInfo {
                balance: U256::from_be_slice(&account.balance.to_be_bytes()),
                nonce: account.nonce,
                code_hash: if account.code.is_empty() {
                    KECCAK_EMPTY
                } else {
                    code.hash_slow()
                },
                account_id: None,
                code: Some(code),
            },
        );
        for (slot, value) in &account.storage {
            db.insert_account_storage(
                Address::new(*addr),
                U256::from_be_slice(slot),
                U256::from_be_slice(value),
            )
            .expect("cache storage insert");
        }
    }
    db
}

fn overlay_cache(world: &mut OvlEvmWorld, db: &CacheDB<EmptyDB>) {
    for (addr, account) in db.cache.accounts.iter() {
        let key = addr.into_array();
        let code_bytes = resolve_code(db, &account.info).original_bytes().to_vec();
        let mut storage = world
            .accounts
            .get(&key)
            .map(|existing| existing.storage.clone())
            .unwrap_or_default();
        for (slot, value) in account.storage.iter() {
            let slot_bytes = slot.to_be_bytes::<32>();
            if value.is_zero() {
                storage.remove(&slot_bytes);
            } else {
                storage.insert(slot_bytes, value.to_be_bytes::<32>());
            }
        }
        let balance = OvlWei::from_be_bytes(account.info.balance.to_be_bytes());
        if balance.is_zero()
            && account.info.nonce == 0
            && code_bytes.is_empty()
            && storage.is_empty()
        {
            world.accounts.remove(&key);
            continue;
        }
        world.accounts.insert(
            key,
            EvmAccount {
                balance,
                nonce: account.info.nonce,
                code: code_bytes,
                storage,
            },
        );
    }
}

fn resolve_code(db: &CacheDB<EmptyDB>, info: &AccountInfo) -> Bytecode {
    if let Some(code) = &info.code {
        if !code.is_empty() || info.code_hash == KECCAK_EMPTY {
            return code.clone();
        }
    }
    db.cache
        .contracts
        .get(&info.code_hash)
        .cloned()
        .unwrap_or_else(Bytecode::default)
}
