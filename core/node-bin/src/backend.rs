//! Live [`RpcBackend`] backed by the node state store and mempool.

use std::collections::HashMap;
use std::sync::Arc;

use agora_p2p::{Mempool, NetworkHandle, NetworkMessage};
use agora_rpc::{RpcBackend, RpcError};
use agora_state_machine::{meta_keys, ColumnFamily, StateStore};
use agora_types::{Address, Amount, Block, Hash, Transaction, TxOut};
use borsh::BorshDeserialize;

/// Node RPC surface: tips/blocks from store, signed tx → mempool + gossip.
pub struct NodeBackend {
    store: Arc<StateStore>,
    mempool: Mempool,
    net: Option<NetworkHandle>,
    /// When true, `agora_fundAddress` credits an overlay balance (testnet only).
    allow_fund: bool,
    fund_overlay: HashMap<Address, Amount>,
}

impl NodeBackend {
    pub fn new(store: Arc<StateStore>, net: Option<NetworkHandle>, allow_fund: bool) -> Self {
        Self {
            store,
            mempool: Mempool::new(10_000),
            net,
            allow_fund,
            fund_overlay: HashMap::new(),
        }
    }

    fn load_tips(&self) -> Result<Vec<Hash>, RpcError> {
        let bytes = self
            .store
            .get_cf(ColumnFamily::Meta, meta_keys::TIPS)
            .map_err(|e| RpcError::Internal(e.to_string()))?
            .unwrap_or_default();
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        borsh::from_slice(&bytes).map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn load_block(&self, hash: &Hash) -> Result<Option<Block>, RpcError> {
        for cf in [ColumnFamily::Hot, ColumnFamily::Warm, ColumnFamily::Archival] {
            if let Some(bytes) = self
                .store
                .get_cf(cf, hash.as_bytes())
                .map_err(|e| RpcError::Internal(e.to_string()))?
            {
                let block = Block::try_from_slice(&bytes)
                    .map_err(|e| RpcError::Internal(e.to_string()))?;
                return Ok(Some(block));
            }
        }
        Ok(None)
    }

    fn utxo_balance(&self, address: &Address) -> Result<Amount, RpcError> {
        let mut total = Amount::ZERO;
        self.store
            .for_each_cf(ColumnFamily::Utxo, |_key, value| {
                let out = TxOut::try_from_slice(value)
                    .map_err(|e| agora_state_machine::StateError::Storage(e.to_string()))?;
                if &out.address == address {
                    total = total.checked_add(out.value).ok_or_else(|| {
                        agora_state_machine::StateError::Storage("balance overflow".into())
                    })?;
                }
                Ok(())
            })
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        Ok(total)
    }
}

impl RpcBackend for NodeBackend {
    fn dag_tips(&self) -> Vec<Hash> {
        self.load_tips().unwrap_or_default()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        self.load_block(hash).ok().flatten()
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError> {
        let id = self
            .mempool
            .admit(tx.clone())
            .map_err(|e| RpcError::Rejected(e.to_string()))?;
        if let Some(net) = &self.net {
            if let Err(err) = net.publish_message(NetworkMessage::Transaction(tx)) {
                return Err(RpcError::Internal(err.to_string()));
            }
        }
        Ok(id)
    }

    fn get_balance(&self, address: &Address) -> Amount {
        let utxo = self.utxo_balance(address).unwrap_or(Amount::ZERO);
        let overlay = self
            .fund_overlay
            .get(address)
            .copied()
            .unwrap_or(Amount::ZERO);
        utxo.checked_add(overlay).unwrap_or(utxo)
    }

    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError> {
        if !self.allow_fund {
            return Err(RpcError::Rejected(
                "agora_fundAddress disabled (set AGORA_RPC_ALLOW_FUND=1 for testnet)".into(),
            ));
        }
        if amount.as_base_units() == 0 {
            return Err(RpcError::InvalidParams("amount must be > 0".into()));
        }
        let entry = self.fund_overlay.entry(address).or_insert(Amount::ZERO);
        *entry = entry
            .checked_add(amount)
            .ok_or_else(|| RpcError::Internal("balance overflow".into()))?;
        Ok(self.get_balance(&address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_state_machine::GenesisBuilder;
    use agora_types::Address;

    #[test]
    fn genesis_tips_and_premine_balance() {
        let store = Arc::new(StateStore::open("/tmp/agora-node-backend-test").unwrap());
        let premine = Address([9u8; 20]);
        let hash = GenesisBuilder::default()
            .with_premine_address(premine)
            .ignite(&store)
            .unwrap();

        let backend = NodeBackend::new(store, None, false);
        assert_eq!(backend.dag_tips(), vec![hash]);
        assert!(backend.get_block(&hash).is_some());
        assert_eq!(
            backend.get_balance(&premine).as_base_units(),
            Amount::from_whole(10_000_000).unwrap().as_base_units()
        );
        assert!(matches!(
            NodeBackend::new(
                Arc::new(StateStore::open("/tmp/agora-node-backend-test2").unwrap()),
                None,
                false
            )
            .fund_address(premine, Amount::from_base_units(1)),
            Err(RpcError::Rejected(_))
        ));
    }
}
