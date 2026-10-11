//! Separate mempool for version-2 raw Ethereum envelopes.
//!
//! These are not Agora-signed. Authorization is the secp256k1 signature inside
//! the RLP bytes. They must not reserve the OVL account-nonce slot used by
//! `OvlExecution` v1.

use agora_types::{OvlExecutionTx, OVL_EXECUTION_RAW_EVM_VERSION};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub(super) fn ovl_raw_maps_len(&self) -> usize {
        self.ovl_raw_execution_txs.len()
    }

    pub(super) fn ovl_raw_maps_contains(&self, tx_id: &agora_types::Hash) -> bool {
        self.ovl_raw_execution_txs.contains_key(tx_id)
    }

    pub fn get_ovl_raw_execution(&self, tx_id: &agora_types::Hash) -> Option<&OvlExecutionTx> {
        self.ovl_raw_execution_txs.get(tx_id)
    }

    /// Admit a pre-validated raw Ethereum envelope. Callers parse `data` first.
    pub fn admit_ovl_raw_execution(
        &mut self,
        tx: OvlExecutionTx,
    ) -> Result<agora_types::Hash, P2pError> {
        if tx.version != OVL_EXECUTION_RAW_EVM_VERSION {
            return Err(P2pError::MempoolRejected(
                "OVL raw gossip requires execution version 2".into(),
            ));
        }
        if tx.data.is_empty() {
            return Err(P2pError::MempoolRejected(
                "OVL raw gossip missing Ethereum payload".into(),
            ));
        }
        if !tx.public_key.is_empty() || !tx.signature.is_empty() {
            return Err(P2pError::MempoolRejected(
                "OVL raw gossip must not carry Agora signatures".into(),
            ));
        }
        let id = tx.tx_id();
        if self.ovl_raw_execution_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        self.ovl_raw_execution_txs.insert(id, tx);
        Ok(id)
    }

    pub fn select_ovl_raw_executions(&self, max: usize) -> Vec<OvlExecutionTx> {
        let mut txs: Vec<_> = self.ovl_raw_execution_txs.values().cloned().collect();
        txs.sort_by_key(OvlExecutionTx::tx_id);
        txs.truncate(max);
        txs
    }

    pub(super) fn evict_ovl_raw(&mut self, tx: &OvlExecutionTx) {
        let id = tx.tx_id();
        self.ovl_raw_execution_txs.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mempool;

    #[test]
    fn raw_evm_has_its_own_pool() {
        let tx = OvlExecutionTx::raw_ethereum(vec![0x02, 0xc0]);
        let mut pool = Mempool::new(8);
        let id = pool.admit_ovl_raw_execution(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        assert_eq!(pool.select_ovl_raw_executions(8), vec![tx.clone()]);
        assert!(pool.admit_ovl_execution(tx.clone()).is_err());
    }

    #[test]
    fn agora_signed_version_is_rejected_from_raw_pool() {
        let tx = OvlExecutionTx::unsigned(
            agora_types::Address([1; 20]),
            agora_types::Address([2; 20]),
            agora_types::Amount::from_base_units(1),
            21_000,
            1,
            0,
            vec![],
        );
        let mut pool = Mempool::new(8);
        assert!(pool.admit_ovl_raw_execution(tx).is_err());
    }
}
