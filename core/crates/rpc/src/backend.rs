use std::collections::HashMap;

use agora_types::{Address, Amount, Block, BlockHeader, Hash, OutPoint, Transaction, TxOut};

use crate::error::RpcError;

/// Spendable UTXO returned by `agora_getUtxos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtxoEntry {
    pub outpoint: OutPoint,
    pub value: Amount,
}

/// Mempool / confirmed / missing status for `agora_getTransaction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxStatus {
    Pending,
    Confirmed,
    Unknown,
}

impl TxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Unknown => "unknown",
        }
    }
}

/// Result of looking up a transaction by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxLookup {
    pub tx_id: Hash,
    pub status: TxStatus,
    pub block_id: Option<Hash>,
    pub index: Option<u32>,
    pub fee: Option<u64>,
    /// Blue-score depth vs best tip (`tip − block + 1`) when confirmed; else `None`.
    pub confirmations: Option<u64>,
    pub transaction: Option<Transaction>,
}

/// One pending mempool entry for `agora_getMempool`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolEntry {
    pub tx_id: Hash,
    pub fee: Option<u64>,
    pub transaction: Transaction,
}

/// Operator-facing node snapshot for `agora_getNodeInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfo {
    pub network: String,
    pub version: String,
    pub peer_id: Option<String>,
    pub connected_peers: Option<u32>,
    pub tip_count: usize,
    pub mempool_count: usize,
    pub pow_algorithm: String,
    pub bits: u32,
    pub archival: bool,
    pub hot_window: u32,
    pub allow_fund: bool,
    /// Bech32m miner payout address when known.
    pub miner_address: Option<String>,
}

impl TxLookup {
    pub fn unknown(tx_id: Hash) -> Self {
        Self {
            tx_id,
            status: TxStatus::Unknown,
            block_id: None,
            index: None,
            fee: None,
            confirmations: None,
            transaction: None,
        }
    }

    pub fn pending(tx: Transaction, fee: Option<u64>) -> Self {
        Self {
            tx_id: tx.tx_id(),
            status: TxStatus::Pending,
            block_id: None,
            index: None,
            fee,
            confirmations: None,
            transaction: Some(tx),
        }
    }

    pub fn confirmed(tx: Transaction, block_id: Hash, index: u32, confirmations: u64) -> Self {
        Self {
            tx_id: tx.tx_id(),
            status: TxStatus::Confirmed,
            block_id: Some(block_id),
            index: Some(index),
            fee: None,
            confirmations: Some(confirmations.max(1)),
            transaction: Some(tx),
        }
    }
}

/// Node-facing surface the RPC dispatcher calls into.
pub trait RpcBackend: Send {
    fn dag_tips(&self) -> Vec<Hash>;
    fn get_block(&self, hash: &Hash) -> Option<Block>;
    /// Mempool → confirmed index → unknown (never hard-errors on missing).
    fn get_transaction(&self, tx_id: &Hash) -> Result<TxLookup, RpcError>;
    /// Pending mempool snapshot (fee-desc, then `tx_id`), capped by `limit`.
    fn get_mempool(&self, limit: usize) -> Result<Vec<MempoolEntry>, RpcError>;
    /// Local node / storage / tip snapshot for explorers and operators.
    fn get_node_info(&self) -> Result<NodeInfo, RpcError>;
    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError>;
    fn get_balance(&self, address: &Address) -> Amount;
    /// Live UTXO set for wallet coin selection.
    fn get_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError>;
    /// Testnet / faucet credit path. Production node backends may reject this.
    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError>;
    /// Mining template (header + coinbase txs) for the CPU sidecar / stratum.
    fn get_block_template(&self) -> Result<Block, RpcError>;
    /// Admit a mined block after PoW verification (node) or local insert (tests).
    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError>;
}

/// In-memory ledger used by unit tests and the local faucet scaffold.
#[derive(Debug, Default)]
pub struct InMemoryBackend {
    tips: Vec<Hash>,
    blocks: HashMap<Hash, Block>,
    balances: HashMap<Address, Amount>,
    utxos: HashMap<OutPoint, TxOut>,
    mempool: HashMap<Hash, Transaction>,
    /// `tx_id` → `(block_id, index)` for confirmed txs.
    tx_index: HashMap<Hash, (Hash, u32)>,
    template_bits: u32,
    fund_nonce: u64,
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
        for (index, tx) in block.transactions.iter().enumerate() {
            let tx_id = tx.tx_id();
            self.tx_index.insert(tx_id, (id, index as u32));
            self.mempool.remove(&tx_id);
        }
        self.blocks.insert(id, block);
    }

    /// Shortest tip→block parent distance + 1 (BFS). `None` if unreachable.
    fn tip_confirmations(&self, block_id: &Hash) -> Option<u64> {
        use std::collections::{HashSet, VecDeque};
        let mut best: Option<u64> = None;
        for tip in &self.tips {
            let mut q = VecDeque::from([(*tip, 0u64)]);
            let mut seen = HashSet::from([*tip]);
            while let Some((hash, dist)) = q.pop_front() {
                if hash == *block_id {
                    let conf = dist + 1;
                    best = Some(best.map_or(conf, |b| b.min(conf)));
                    break;
                }
                if let Some(block) = self.blocks.get(&hash) {
                    for parent in &block.header.parents {
                        if seen.insert(*parent) {
                            q.push_back((*parent, dist + 1));
                        }
                    }
                }
            }
        }
        best
    }
}

impl RpcBackend for InMemoryBackend {
    fn dag_tips(&self) -> Vec<Hash> {
        self.tips.clone()
    }

    fn get_block(&self, hash: &Hash) -> Option<Block> {
        self.blocks.get(hash).cloned()
    }

    fn get_transaction(&self, tx_id: &Hash) -> Result<TxLookup, RpcError> {
        if let Some(tx) = self.mempool.get(tx_id) {
            return Ok(TxLookup::pending(tx.clone(), None));
        }
        if let Some((block_id, index)) = self.tx_index.get(tx_id).copied() {
            if let Some(block) = self.blocks.get(&block_id) {
                if let Some(tx) = block.transactions.get(index as usize) {
                    // In-memory backend: tip distance via ancestor walk when possible.
                    let confirmations = self.tip_confirmations(&block_id).unwrap_or(1);
                    return Ok(TxLookup::confirmed(
                        tx.clone(),
                        block_id,
                        index,
                        confirmations,
                    ));
                }
            }
        }
        Ok(TxLookup::unknown(*tx_id))
    }

    fn get_mempool(&self, limit: usize) -> Result<Vec<MempoolEntry>, RpcError> {
        let mut entries: Vec<MempoolEntry> = self
            .mempool
            .iter()
            .map(|(tx_id, tx)| MempoolEntry {
                tx_id: *tx_id,
                fee: None,
                transaction: tx.clone(),
            })
            .collect();
        entries.sort_by(|a, b| a.tx_id.as_bytes().cmp(b.tx_id.as_bytes()));
        if entries.len() > limit {
            entries.truncate(limit);
        }
        Ok(entries)
    }

    fn get_node_info(&self) -> Result<NodeInfo, RpcError> {
        Ok(NodeInfo {
            network: "agora".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            peer_id: None,
            connected_peers: None,
            tip_count: self.tips.len(),
            mempool_count: self.mempool.len(),
            pow_algorithm: "test".into(),
            bits: self.template_bits,
            archival: true,
            hot_window: 0,
            allow_fund: true,
            miner_address: None,
        })
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

    fn get_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError> {
        Ok(self
            .utxos
            .iter()
            .filter(|(_, out)| &out.address == address)
            .map(|(op, out)| UtxoEntry {
                outpoint: *op,
                value: out.value,
            })
            .collect())
    }

    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError> {
        if amount.as_base_units() == 0 {
            return Err(RpcError::InvalidParams("amount must be > 0".into()));
        }
        self.fund_nonce = self.fund_nonce.saturating_add(1);
        let tx_id = Hash::hash_borsh(&(
            b"agora_fund_mem",
            address,
            amount.as_base_units(),
            self.fund_nonce,
        ));
        let op = OutPoint {
            tx_id,
            index: 0,
        };
        self.utxos.insert(
            op,
            TxOut {
                value: amount,
                address,
            },
        );
        let entry = self.balances.entry(address).or_insert(Amount::ZERO);
        *entry = entry
            .checked_add(amount)
            .ok_or_else(|| RpcError::Internal("balance overflow".into()))?;
        Ok(*entry)
    }

    fn get_block_template(&self) -> Result<Block, RpcError> {
        Ok(Block {
            header: BlockHeader {
                version: 1,
                parents: self.tips.clone(),
                timestamp_ms: 0,
                bits: self.template_bits,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
        })
    }

    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError> {
        let id = block.id();
        self.insert_block(block);
        Ok(id)
    }
}
