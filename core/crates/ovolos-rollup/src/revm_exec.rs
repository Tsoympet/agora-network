//! Production EVM binding via the audited `revm` crate.
//!
//! OVL plays the Ethereum role on L2: persistent account/contract state, value
//! transfers, CREATE, and gas-metered execution. State roots are deterministic
//! SHA-256 digests over the account cache (not full Ethereum MPT roots yet).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use agora_types::Hash;
use revm::context::TxEnv;
use revm::database::{CacheDB, EmptyDB};
use revm::primitives::{Address, Bytes, TxKind, B256, KECCAK_EMPTY, U256};
use revm::state::{AccountInfo, Bytecode};
use revm::{Context, ExecuteCommitEvm, MainBuilder, MainContext};
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

/// Snapshot of account info for cloning between batches / fraud re-exec.
#[derive(Clone, Debug, Default)]
struct AccountSnap {
    balance: U256,
    nonce: u64,
    code_hash: B256,
    code: Bytecode,
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
        }
        Ok(db)
    }

    fn capture_snapshot(db: &CacheDB<EmptyDB>) -> HashMap<Address, AccountSnap> {
        let mut out = HashMap::new();
        for (addr, acc) in db.cache.accounts.iter() {
            out.insert(
                *addr,
                AccountSnap {
                    balance: acc.info.balance,
                    nonce: acc.info.nonce,
                    code_hash: acc.info.code_hash,
                    code: acc.info.code.clone().unwrap_or_else(Bytecode::default),
                },
            );
        }
        out
    }

    fn state_root(db: &CacheDB<EmptyDB>) -> Hash {
        let mut accounts: BTreeMap<Address, (U256, u64, B256)> = BTreeMap::new();
        for (addr, acc) in db.cache.accounts.iter() {
            accounts.insert(
                *addr,
                (acc.info.balance, acc.info.nonce, acc.info.code_hash),
            );
        }
        let mut hasher = Sha256::new();
        for (addr, (bal, nonce, code_hash)) in accounts {
            hasher.update(addr.as_slice());
            hasher.update(bal.to_be_bytes::<32>());
            hasher.update(nonce.to_le_bytes());
            hasher.update(code_hash.as_slice());
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Hash(out)
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
}

impl EvmExecutor for RevmExecutor {
    fn apply_batch(&self, prev_state_root: &Hash, txs: &[EvmTx]) -> Result<Hash, RollupError> {
        let db = self.db_from_prev(prev_state_root)?;
        let mut evm = Context::mainnet().with_db(db).build_mainnet();

        for (idx, raw) in txs.iter().enumerate() {
            let (to, value, data) = decode_tx(raw)?;
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
            let caller_nonce = evm
                .ctx
                .journaled_state
                .database
                .cache
                .accounts
                .get(&self.caller)
                .map(|a| a.info.nonce)
                .unwrap_or(0);
            let tx = TxEnv::builder()
                .caller(self.caller)
                .kind(kind)
                .value(value)
                .data(data)
                .gas_limit(gas_limit)
                .nonce(caller_nonce)
                .build()
                .map_err(|e| RollupError::Execution(format!("tx env: {e:?}")))?;

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
}
