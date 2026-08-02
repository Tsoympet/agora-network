//! Production EVM binding via the audited `revm` crate.

use std::collections::BTreeMap;

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

/// Compact transfer encoding used by Ovolos tests / scaffolding:
/// `to (20 bytes) || value (32 bytes BE) || calldata (optional)`
pub fn encode_transfer(to: [u8; 20], value: U256, data: &[u8]) -> EvmTx {
    let mut raw = Vec::with_capacity(52 + data.len());
    raw.extend_from_slice(&to);
    raw.extend_from_slice(&value.to_be_bytes::<32>());
    raw.extend_from_slice(data);
    EvmTx(raw)
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

/// `revm`-backed executor that applies value transfers / calls and returns a
/// deterministic account-state root.
#[derive(Debug)]
pub struct RevmExecutor {
    /// Default sender funded at the start of each batch.
    pub caller: Address,
}

impl Default for RevmExecutor {
    fn default() -> Self {
        Self {
            caller: Address::new([0xA1; 20]),
        }
    }
}

impl RevmExecutor {
    pub fn new(caller: Address) -> Self {
        Self { caller }
    }

    fn seed_db(&self, prev_state_root: &Hash) -> CacheDB<EmptyDB> {
        let mut db = CacheDB::new(EmptyDB::default());
        // Fund caller generously; prev root is mixed into an auxiliary account
        // so batch roots remain domain-separated across histories.
        db.insert_account_info(
            self.caller,
            AccountInfo {
                balance: U256::from(10u64).pow(U256::from(24)),
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                account_id: None,
                code: Some(Bytecode::default()),
            },
        );
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
        db
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
}

impl EvmExecutor for RevmExecutor {
    fn apply_batch(&self, prev_state_root: &Hash, txs: &[EvmTx]) -> Result<Hash, RollupError> {
        let db = self.seed_db(prev_state_root);
        let mut evm = Context::mainnet().with_db(db).build_mainnet();

        for (idx, raw) in txs.iter().enumerate() {
            let (to, value, data) = decode_tx(raw)?;
            let gas_limit = if data.is_empty() { 21_000 } else { 300_000 };
            let tx = TxEnv::builder()
                .caller(self.caller)
                .kind(TxKind::Call(to))
                .value(value)
                .data(data)
                .gas_limit(gas_limit)
                .nonce(idx as u64)
                .build()
                .map_err(|e| RollupError::Execution(format!("tx env: {e:?}")))?;

            evm.transact_commit(tx).map_err(|e| {
                RollupError::Execution(format!("revm tx {idx} failed: {e:?}"))
            })?;
        }

        let db_ref = &evm.ctx.journaled_state.database;
        Ok(Self::state_root(db_ref))
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
}
