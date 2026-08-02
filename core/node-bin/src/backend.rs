//! Live [`RpcBackend`] backed by chain admission + mempool.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agora_p2p::{Mempool, NetworkHandle, NetworkMessage};
use agora_rpc::{RpcBackend, RpcError};
use agora_state_machine::{ColumnFamily, StateStore};
use agora_types::{Address, Amount, Block, BlockHeader, Hash, Transaction, TxOut};
use borsh::BorshDeserialize;

use crate::admit::ChainState;

/// Node RPC surface: tips/blocks from store, signed tx → mempool + gossip.
pub struct NodeBackend {
    chain: Arc<Mutex<ChainState>>,
    store: Arc<StateStore>,
    mempool: Arc<Mutex<Mempool>>,
    net: Option<NetworkHandle>,
    /// When true, `agora_fundAddress` credits an overlay balance (testnet only).
    allow_fund: bool,
    fund_overlay: HashMap<Address, Amount>,
}

impl NodeBackend {
    pub fn new(
        chain: Arc<Mutex<ChainState>>,
        store: Arc<StateStore>,
        net: Option<NetworkHandle>,
        allow_fund: bool,
        mempool: Arc<Mutex<Mempool>>,
    ) -> Self {
        Self {
            chain,
            store,
            mempool,
            net,
            allow_fund,
            fund_overlay: HashMap::new(),
        }
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
        self.chain
            .lock()
            .ok()
            .and_then(|g| g.tips().ok())
            .unwrap_or_default()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        self.chain
            .lock()
            .ok()
            .and_then(|g| g.load_block(hash).ok())
            .flatten()
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError> {
        let id = self
            .mempool
            .lock()
            .map_err(|_| RpcError::Internal("mempool lock poisoned".into()))?
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

    fn get_block_template(&self) -> Result<BlockHeader, RpcError> {
        self.chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .block_template()
            .map_err(|e| RpcError::Internal(e.to_string()))
    }

    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError> {
        let id = self
            .chain
            .lock()
            .map_err(|_| RpcError::Internal("chain lock poisoned".into()))?
            .admit_block(block.clone())
            .map_err(|e| match e {
                crate::admit::AdmitError::InvalidPow => {
                    RpcError::Rejected("invalid proof of work".into())
                }
                crate::admit::AdmitError::Duplicate(h) => {
                    RpcError::Rejected(format!("duplicate block {h}"))
                }
                crate::admit::AdmitError::MissingParent(h) => {
                    RpcError::Rejected(format!("missing parent {h}"))
                }
                crate::admit::AdmitError::Utxo(msg) => {
                    RpcError::Rejected(format!("utxo: {msg}"))
                }
                other => RpcError::Internal(other.to_string()),
            })?;
        if let Some(net) = &self.net {
            // Prefer compact + announce; peers inflate from mempool or issue GetBlock.
            let _ = net.publish_message(NetworkMessage::compact_from_block(&block));
            let _ = net.publish_message(NetworkMessage::BlockAnnounce { hash: id });
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::ChainState;
    use agora_consensus::{PowAlgorithm, PowHasher, PowVerifier, RandomXPowHasher};
    use agora_state_machine::GenesisBuilder;
    use agora_types::{Address, Block};

    #[test]
    fn genesis_tips_and_admit_easy_block() {
        let store = Arc::new(StateStore::open("/tmp/agora-node-backend-admit").unwrap());
        let mempool = Arc::new(Mutex::new(Mempool::new(64)));
        let premine = Address([9u8; 20]);
        let genesis = GenesisBuilder::default()
            .with_premine_address(premine)
            .ignite(&store)
            .unwrap();

        let chain = Arc::new(Mutex::new(
            ChainState::bootstrap(store.clone(), genesis, PowAlgorithm::RandomX, 0).unwrap(),
        ));
        let mut backend = NodeBackend::new(chain.clone(), store, None, false, mempool);
        assert_eq!(backend.dag_tips(), vec![genesis]);
        assert_eq!(
            backend.get_balance(&premine).as_base_units(),
            Amount::from_whole(10_000_000).unwrap().as_base_units()
        );

        let mut header = backend.get_block_template().unwrap();
        // bits=0 accepts any RandomX digest.
        header.bits = 0;
        header.nonce = 1;
        let block = Block {
            header: header.clone(),
            transactions: vec![],
        };
        let pow = RandomXPowHasher.pow_hash(&header);
        agora_consensus::LeadingZeroPow::new(PowAlgorithm::RandomX)
            .verify(&header, &pow)
            .unwrap();
        let id = backend.submit_block(block).unwrap();
        assert_ne!(id, genesis);
        assert!(backend.dag_tips().contains(&id));
    }
}
