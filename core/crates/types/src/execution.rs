//! Signed OVL execution envelope for Trident L1.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Amount, Hash};

/// Domain separator for network-bound OVL execution signatures.
pub const OVL_EXECUTION_SIGNING_DOMAIN: &[u8] = b"agora-trident-ovl-execution-v1";

/// Signed account-based OVL value transfer or execution request.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
pub struct OvlExecutionTx {
    pub version: u32,
    pub from: Address,
    /// `Address::ZERO` is reserved for a future contract-create operation.
    pub to: Address,
    pub value: Amount,
    pub gas_limit: u64,
    pub max_fee_per_gas: u64,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl OvlExecutionTx {
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            OVL_EXECUTION_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.from,
            self.to,
            self.value,
            self.gas_limit,
            self.max_fee_per_gas,
            self.nonce,
            &self.data,
        );
        borsh::to_vec(&body).expect("borsh serialize OVL execution body")
    }

    /// Hashes the complete signed envelope, including authorization material.
    pub fn tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn unsigned(
        from: Address,
        to: Address,
        value: Amount,
        gas_limit: u64,
        max_fee_per_gas: u64,
        nonce: u64,
        data: Vec<u8>,
    ) -> Self {
        Self {
            version: 1,
            from,
            to,
            value,
            gas_limit,
            max_fee_per_gas,
            nonce,
            data,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ovl_execution_id_is_stable_and_covers_auth() {
        let mut tx = OvlExecutionTx::unsigned(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            40_000,
            5,
            6,
            vec![0xaa, 0xbb],
        );
        let unsigned_id = tx.tx_id();
        assert_eq!(
            unsigned_id,
            Hash::from_hex("8213a64c639d9a61b26b58342bafe59a4086808f07e6109c62c3c834fc1bc557")
                .expect("locked execution transaction id")
        );

        tx.signature = vec![7u8; 64];
        assert_ne!(tx.tx_id(), unsigned_id);
    }
}
