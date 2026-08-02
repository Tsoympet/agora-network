use std::collections::HashMap;

use agora_types::{Address, Amount, Block, BlockHeader, Hash, Transaction};

use crate::error::RpcError;

/// Node-facing surface the RPC dispatcher calls into.
pub trait RpcBackend: Send {
    fn dag_tips(&self) -> Vec<Hash>;
    fn get_block(&self, hash: &Hash) -> Option<Block>;
    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError>;
    fn get_balance(&self, address: &Address) -> Amount;
    /// Testnet / faucet credit path. Production node backends may reject this.
    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError>;
    /// Mining template for the CPU sidecar / stratum.
    fn get_block_template(&self) -> Result<BlockHeader, RpcError>;
    /// Admit a mined block after PoW verification (node) or local insert (tests).
    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError>;
}

/// In-memory ledger used by unit tests and the local faucet scaffold.
#[derive(Debug, Default)]
pub struct InMemoryBackend {
    tips: Vec<Hash>,
    blocks: HashMap<Hash, Block>,
    balances: HashMap<Address, Amount>,
    mempool: HashMap<Hash, Transaction>,
    template_bits: u32,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            template_bits: 0,
            ..Self::default()
        }
    }

    pub fn set_tips(&mut self, tips: Vec<Hash>) {
        self.tips = tips;
    }

    pub fn insert_block(&mut self, block: Block) {
        let id = block.id();
        if !self.tips.contains(&id) {
            self.tips
                .retain(|t| !block.header.parents.iter().any(|p| p == t));
            self.tips.push(id);
        }
        self.blocks.insert(id, block);
    }
}

impl RpcBackend for InMemoryBackend {
    fn dag_tips(&self) -> Vec<Hash> {
        self.tips.clone()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        self.blocks.get(hash).cloned()
    }

    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError> {
        if tx.outputs.is_empty() {
            return Err(RpcError::Rejected("transaction has no outputs".into()));
        }
        let id = tx.tx_id();
        self.mempool.insert(id, tx);
        Ok(id)
    }

    fn get_balance(&self, address: &Address) -> Amount {
        self.balances
            .get(address)
            .copied()
            .unwrap_or(Amount::ZERO)
    }

    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError> {
        if amount.as_base_units() == 0 {
            return Err(RpcError::InvalidParams("amount must be > 0".into()));
        }
        let entry = self.balances.entry(address).or_insert(Amount::ZERO);
        *entry = entry
            .checked_add(amount)
            .ok_or_else(|| RpcError::Internal("balance overflow".into()))?;
        Ok(*entry)
    }

    fn get_block_template(&self) -> Result<BlockHeader, RpcError> {
        Ok(BlockHeader {
            version: 1,
            parents: self.tips.clone(),
            timestamp_ms: 0,
            bits: self.template_bits,
            nonce: 0,
            tx_root: Hash::ZERO,
        })
    }

    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError> {
        let id = block.id();
        self.insert_block(block);
        Ok(id)
    }
}
