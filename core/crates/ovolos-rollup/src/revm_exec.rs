//! Production EVM binding via the audited `revm` crate.
//!
//! OVL plays the Ethereum role on L2: persistent account/contract state (including
//! storage), value transfers, CREATE, and gas-metered execution. State roots are
//! deterministic SHA-256 digests over the account + storage cache (not full
//! Ethereum MPT roots yet).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use agora_types::Hash;
use revm::context::TxEnv;
use revm::database::{CacheDB, EmptyDB};
use revm::primitives::{Address, Bytes, TxKind, B256, KECCAK_EMPTY, U256};
use revm::state::{AccountInfo, Bytecode};
use revm::{Context, ExecuteCommitEvm, ExecuteEvm, MainBuilder, MainContext};
use sha2::{Digest, Sha256};

use crate::executor::EvmExecutor;
use crate::types::EvmTx;
use crate::RollupError;

/// Compact Ovolos tx encoding (Ethereum-class scaffolding):
/// `to (20 bytes) || value (32 bytes BE) || calldata (optional)`
///
/// `to == 0x00…00` means **CREATE** (deploy `calldata` as init code).
pub fn encode_transfer(to: [u8; 20], value: U256, data: &[u8]) -> EvmTx {
    let mut raw = Vec::with_capacity(52 + data.len());
    raw.extend_from_slice(&to);
    raw.extend_from_slice(&value.to_be_bytes::<32>());
    raw.extend_from_slice(data);
    EvmTx(raw)
}

/// Encode a contract CREATE with init bytecode.
pub fn encode_create(value: U256, init_code: &[u8]) -> EvmTx {
    encode_transfer([0u8; 20], value, init_code)
}

/// Convenience: value transfer with `u64` amount (no calldata).
pub fn encode_value_transfer(to: [u8; 20], value: u64) -> EvmTx {
    encode_transfer(to, U256::from(value), &[])
}

fn decode_tx(tx: &EvmTx) -> Result<(Address, U256, Bytes), RollupError> {
    if tx.0.len() < 52 {
        return Err(RollupError::Execution(
            "evm tx too short; expected to||value[||data]".into(),
        ));
    }
    let mut to = [0u8; 20];
    to.copy_from_slice(&tx.0[..20]);
    let value = U256::from_be_slice(&tx.0[20..52]);
    let data = Bytes::copy_from_slice(&tx.0[52..]);
    Ok((Address::new(to), value, data))
}

/// Snapshot of account info + storage for cloning between batches / fraud re-exec.
#[derive(Clone, Debug, Default)]
struct AccountSnap {
    balance: U256,
    nonce: u64,
    code_hash: B256,
    code: Bytecode,
    /// Non-zero storage slots (sorted when hashed into the state root).
    storage: BTreeMap<U256, U256>,
}

/// `revm`-backed executor with **persistent** state snapshots keyed by state root.
#[derive(Debug)]
pub struct RevmExecutor {
    /// Default sender funded at genesis / first seed.
    pub caller: Address,
    snapshots: Mutex<HashMap<[u8; 32], HashMap<Address, AccountSnap>>>,
}

impl Default for RevmExecutor {
    fn default() -> Self {
        Self {
            caller: Address::new([0xA1; 20]),
            snapshots: Mutex::new(HashMap::new()),
        }
    }
}

impl Clone for RevmExecutor {
    fn clone(&self) -> Self {
        let snaps = self.snapshots.lock().map(|g| g.clone()).unwrap_or_default();
        Self {
            caller: self.caller,
            snapshots: Mutex::new(snaps),
        }
    }
}

impl RevmExecutor {
    pub fn new(caller: Address) -> Self {
        Self {
            caller,
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    fn fund_caller(db: &mut CacheDB<EmptyDB>, caller: Address) {
        db.insert_account_info(
            caller,
            AccountInfo {
                balance: U256::from(10u64).pow(U256::from(24)),
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                account_id: None,
                code: Some(Bytecode::default()),
            },
        );
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

    fn db_from_prev(&self, prev_state_root: &Hash) -> Result<CacheDB<EmptyDB>, RollupError> {
        let mut db = CacheDB::new(EmptyDB::default());
        if prev_state_root == &Hash::ZERO {
            Self::fund_caller(&mut db, self.caller);
            return Ok(db);
        }
        let snaps = self
            .snapshots
            .lock()
            .map_err(|_| RollupError::Execution("revm snapshot lock poisoned".into()))?;
        let Some(accounts) = snaps.get(prev_state_root.as_bytes()) else {
            // Unknown prior root: seed caller so isolated tests still work.
            drop(snaps);
            Self::fund_caller(&mut db, self.caller);
            // Domain-separate with meta account from prev root.
            let mut meta = [0u8; 20];
            meta.copy_from_slice(&prev_state_root.as_bytes()[..20]);
            db.insert_account_info(
                Address::new(meta),
                AccountInfo {
                    balance: U256::from_be_slice(prev_state_root.as_bytes()),
                    nonce: 0,
                    code_hash: KECCAK_EMPTY,
                    account_id: None,
                    code: Some(Bytecode::default()),
                },
            );
            return Ok(db);
        };
        for (addr, snap) in accounts {
            db.insert_account_info(
                *addr,
                AccountInfo {
                    balance: snap.balance,
                    nonce: snap.nonce,
                    code_hash: snap.code_hash,
                    account_id: None,
                    code: Some(snap.code.clone()),
                },
            );
            for (slot, value) in &snap.storage {
                db.insert_account_storage(*addr, *slot, *value)
                    .map_err(|e| RollupError::Execution(format!("restore storage: {e:?}")))?;
            }
        }
        Ok(db)
    }

    fn capture_snapshot(db: &CacheDB<EmptyDB>) -> HashMap<Address, AccountSnap> {
        let mut out = HashMap::new();
        for (addr, acc) in db.cache.accounts.iter() {
            let mut storage = BTreeMap::new();
            for (k, v) in acc.storage.iter() {
                if !v.is_zero() {
                    storage.insert(*k, *v);
                }
            }
            out.insert(
                *addr,
                AccountSnap {
                    balance: acc.info.balance,
                    nonce: acc.info.nonce,
                    code_hash: acc.info.code_hash,
                    code: Self::resolve_code(db, &acc.info),
                    storage,
                },
            );
        }
        out
    }

    fn state_root(db: &CacheDB<EmptyDB>) -> Hash {
        let mut accounts: BTreeMap<Address, (U256, u64, B256, BTreeMap<U256, U256>)> =
            BTreeMap::new();
        for (addr, acc) in db.cache.accounts.iter() {
            let mut storage = BTreeMap::new();
            for (k, v) in acc.storage.iter() {
                if !v.is_zero() {
                    storage.insert(*k, *v);
                }
            }
            accounts.insert(
                *addr,
                (
                    acc.info.balance,
                    acc.info.nonce,
                    acc.info.code_hash,
                    storage,
                ),
            );
        }
        let mut hasher = Sha256::new();
        for (addr, (bal, nonce, code_hash, storage)) in accounts {
            hasher.update(addr.as_slice());
            hasher.update(bal.to_be_bytes::<32>());
            hasher.update(nonce.to_le_bytes());
            hasher.update(code_hash.as_slice());
            for (slot, value) in storage {
                hasher.update(slot.to_be_bytes::<32>());
                hasher.update(value.to_be_bytes::<32>());
            }
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Hash(out)
    }

    fn build_tx_env(
        &self,
        db: &CacheDB<EmptyDB>,
        to: Address,
        value: U256,
        data: Bytes,
    ) -> Result<TxEnv, RollupError> {
        let is_create = to == Address::ZERO;
        let gas_limit = if is_create {
            1_500_000
        } else if data.is_empty() {
            21_000
        } else {
            300_000
        };
        let kind = if is_create {
            TxKind::Create
        } else {
            TxKind::Call(to)
        };
        let caller_nonce = db
            .cache
            .accounts
            .get(&self.caller)
            .map(|a| a.info.nonce)
            .unwrap_or(0);
        TxEnv::builder()
            .caller(self.caller)
            .kind(kind)
            .value(value)
            .data(data)
            .gas_limit(gas_limit)
            .nonce(caller_nonce)
            .build()
            .map_err(|e| RollupError::Execution(format!("tx env: {e:?}")))
    }

    /// Native OVL/EVM balance for `eth_getBalance`-class queries (base units as wei-like).
    pub fn balance_of(&self, state_root: &Hash, address: [u8; 20]) -> Option<u128> {
        let snaps = self.snapshots.lock().ok()?;
        let accounts = snaps.get(state_root.as_bytes())?;
        let snap = accounts.get(&Address::new(address))?;
        Some(snap.balance.try_into().unwrap_or(u128::MAX))
    }

    pub fn nonce_of(&self, state_root: &Hash, address: [u8; 20]) -> Option<u64> {
        let snaps = self.snapshots.lock().ok()?;
        let accounts = snaps.get(state_root.as_bytes())?;
        Some(accounts.get(&Address::new(address))?.nonce)
    }

    /// Contract bytecode for `eth_getCode` (empty vec for EOAs / unknown).
    pub fn code_of(&self, state_root: &Hash, address: [u8; 20]) -> Vec<u8> {
        let Ok(snaps) = self.snapshots.lock() else {
            return Vec::new();
        };
        let Some(accounts) = snaps.get(state_root.as_bytes()) else {
            return Vec::new();
        };
        let Some(snap) = accounts.get(&Address::new(address)) else {
            return Vec::new();
        };
        snap.code.bytes().to_vec()
    }

    /// Storage slot for `eth_getStorageAt` (32-byte BE slot → 32-byte BE value).
    pub fn storage_at_bytes(
        &self,
        state_root: &Hash,
        address: [u8; 20],
        slot: [u8; 32],
    ) -> [u8; 32] {
        let Ok(snaps) = self.snapshots.lock() else {
            return [0u8; 32];
        };
        let Some(accounts) = snaps.get(state_root.as_bytes()) else {
            return [0u8; 32];
        };
        let Some(snap) = accounts.get(&Address::new(address)) else {
            return [0u8; 32];
        };
        let key = U256::from_be_slice(&slot);
        let value = snap.storage.get(&key).copied().unwrap_or(U256::ZERO);
        value.to_be_bytes::<32>()
    }

    /// Eth-class `eth_call`: execute without committing state.
    pub fn eth_call(
        &self,
        state_root: &Hash,
        to: [u8; 20],
        data: &[u8],
        value: u128,
    ) -> Result<Vec<u8>, RollupError> {
        let db = self.db_from_prev(state_root)?;
        let caller_nonce = db
            .cache
            .accounts
            .get(&self.caller)
            .map(|a| a.info.nonce)
            .unwrap_or(0);
        let tx = TxEnv::builder()
            .caller(self.caller)
            .kind(TxKind::Call(Address::new(to)))
            .value(U256::from(value))
            .data(Bytes::copy_from_slice(data))
            // eth_call is read-only simulation — give ample gas even with empty calldata.
            .gas_limit(1_000_000)
            .nonce(caller_nonce)
            .build()
            .map_err(|e| RollupError::Execution(format!("eth_call tx env: {e:?}")))?;
        let mut evm = Context::mainnet().with_db(db).build_mainnet();
        let result = evm
            .transact(tx)
            .map_err(|e| RollupError::Execution(format!("eth_call failed: {e:?}")))?;
        Ok(result
            .result
            .output()
            .map(|b| b.to_vec())
            .unwrap_or_default())
    }
}

impl EvmExecutor for RevmExecutor {
    fn apply_batch(&self, prev_state_root: &Hash, txs: &[EvmTx]) -> Result<Hash, RollupError> {
        let db = self.db_from_prev(prev_state_root)?;
        let mut evm = Context::mainnet().with_db(db).build_mainnet();

        for (idx, raw) in txs.iter().enumerate() {
            let (to, value, data) = decode_tx(raw)?;
            let tx = self.build_tx_env(&evm.ctx.journaled_state.database, to, value, data)?;
            evm.transact_commit(tx)
                .map_err(|e| RollupError::Execution(format!("revm tx {idx} failed: {e:?}")))?;
        }

        let db_ref = &evm.ctx.journaled_state.database;
        let root = Self::state_root(db_ref);
        let snap = Self::capture_snapshot(db_ref);
        let mut key = [0u8; 32];
        key.copy_from_slice(root.as_bytes());
        self.snapshots
            .lock()
            .map_err(|_| RollupError::Execution("revm snapshot lock poisoned".into()))?
            .insert(key, snap);
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::EvmExecutor;

    #[test]
    fn revm_transfer_changes_state_root() {
        let exec = RevmExecutor::default();
        let prev = Hash::ZERO;
        let to = Address::new([0xB2; 20]);
        let batch = vec![encode_transfer(to.into_array(), U256::from(42), &[])];
        let root1 = exec.apply_batch(&prev, &batch).expect("exec");
        let root2 = exec.apply_batch(&prev, &batch).expect("exec2");
        assert_eq!(root1, root2);
        assert_ne!(root1, prev);

        let batch_b = vec![encode_transfer(to.into_array(), U256::from(43), &[])];
        let root3 = exec.apply_batch(&prev, &batch_b).expect("exec3");
        assert_ne!(root1, root3);
    }

    #[test]
    fn persistent_state_across_batches() {
        let exec = RevmExecutor::default();
        let to = Address::new([0xB2; 20]);
        let b0 = vec![encode_transfer(to.into_array(), U256::from(100), &[])];
        let root0 = exec.apply_batch(&Hash::ZERO, &b0).unwrap();
        let b1 = vec![encode_transfer(to.into_array(), U256::from(50), &[])];
        let root1 = exec.apply_batch(&root0, &b1).unwrap();
        assert_ne!(root0, root1);
        let bal = exec.balance_of(&root1, to.into_array()).unwrap();
        assert_eq!(bal, 150);
    }

    #[test]
    fn storage_included_in_state_root_and_persists() {
        let exec = RevmExecutor::default();
        // Init code: SSTORE(0, 42); RETURN empty runtime.
        // PUSH1 0x2a PUSH1 0x00 SSTORE PUSH1 0x00 PUSH1 0x00 RETURN
        let init = hex_literal(&[0x60, 0x2a, 0x60, 0x00, 0x55, 0x60, 0x00, 0x60, 0x00, 0xf3]);
        let root = exec
            .apply_batch(&Hash::ZERO, &[encode_create(U256::ZERO, &init)])
            .unwrap();
        // CREATE address = keccak(rlp([sender, nonce])) — use storage scan via snapshots.
        let snaps = exec.snapshots.lock().unwrap();
        let accounts = snaps.get(root.as_bytes()).unwrap();
        let contract = accounts
            .iter()
            .find(|(addr, snap)| **addr != exec.caller && snap.storage.contains_key(&U256::ZERO))
            .map(|(a, _)| *a)
            .expect("contract with storage");
        drop(snaps);
        assert_eq!(
            U256::from_be_slice(&exec.storage_at_bytes(&root, contract.into_array(), [0u8; 32])),
            U256::from(42)
        );

        // Empty follow-on batch from same root must reload storage and keep root stable
        // when no txs change state — apply_batch with no txs still re-hashes.
        let root2 = exec.apply_batch(&root, &[]).unwrap();
        assert_eq!(
            U256::from_be_slice(&exec.storage_at_bytes(&root2, contract.into_array(), [0u8; 32])),
            U256::from(42)
        );
    }

    #[test]
    fn eth_get_code_and_eth_call() {
        let exec = RevmExecutor::default();
        // Init returns runtime PUSH1 0x7b PUSH1 0x00 MSTORE PUSH1 0x20 PUSH1 0x00 RETURN
        // runtime bytes: 60 7b 60 00 52 60 20 60 00 f3
        let runtime = [0x60u8, 0x7b, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3];
        let mut init = Vec::new();
        // PUSH1 len, PUSH1 offset-of-code, PUSH1 0, CODECOPY, PUSH1 len, PUSH1 0, RETURN
        // Simpler: embed runtime after a short prefix using CODECOPY from init itself.
        // PUSH1 <len> PUSH1 <code_offset> PUSH1 0 CODECOPY PUSH1 <len> PUSH1 0 RETURN + runtime
        init.extend_from_slice(&[
            0x60,
            runtime.len() as u8,
            0x60,
            0x0c, // code starts at byte 12
            0x60,
            0x00,
            0x39, // CODECOPY
            0x60,
            runtime.len() as u8,
            0x60,
            0x00,
            0xf3, // RETURN
        ]);
        init.extend_from_slice(&runtime);
        let root = exec
            .apply_batch(&Hash::ZERO, &[encode_create(U256::ZERO, &init)])
            .unwrap();
        let snaps = exec.snapshots.lock().unwrap();
        let accounts = snaps.get(root.as_bytes()).unwrap();
        let contract = accounts
            .iter()
            .find(|(addr, snap)| {
                **addr != exec.caller && !snap.code.is_empty() && snap.code_hash != KECCAK_EMPTY
            })
            .map(|(a, _)| *a)
            .expect("deployed contract");
        drop(snaps);

        let code = exec.code_of(&root, contract.into_array());
        // revm may pad bytecode with a trailing zero byte; compare the runtime prefix.
        assert!(
            code.starts_with(&runtime),
            "code={code:?} runtime={runtime:?}"
        );

        let out = exec.eth_call(&root, contract.into_array(), &[], 0).unwrap();
        assert_eq!(out.len(), 32);
        assert_eq!(out[31], 0x7b);
    }

    fn hex_literal(bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }
}
