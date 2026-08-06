use std::collections::HashMap;
use std::sync::Mutex;

use agora_governance::{
    civic_overview_json, list_proposals_json, list_topics_json, office_json, proposal_json,
    CivicSnapshot, ProposalKind, TopicCategory, VoteChoice,
};
use agora_types::{Address, Amount, Block, BlockHeader, Hash, OutPoint, Transaction, TxOut};
use serde_json::{json, Value};

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
    /// Indexed in a block that is red / non-accepted in the current virtual view.
    Orphaned,
    Unknown,
}

impl TxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Orphaned => "orphaned",
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
    /// Explicit acceptance status (`Accepted` / `ConflictLost` / …). Never infer from color alone.
    pub acceptance: Option<String>,
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
    /// Hex id of Block 0 for this datadir / network.
    pub genesis_hash: Option<String>,
    /// Signing domain chain id (`agora-mainnet-1` / `agora-testnet-1` / `agora-dev`).
    pub chain_id: Option<String>,
    /// Minimum mempool relay fee (`in − out`) in base units.
    pub min_relay_fee: u64,
}

/// Fee guidance for wallets (`agora_estimateFee`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeEstimate {
    pub min_relay_fee: u64,
    /// Suggested fee for a typical single-input transfer.
    /// Live nodes use mempool median + congestion premium (Bitcoin-class fee market).
    pub suggested_fee: u64,
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
            acceptance: None,
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
            acceptance: None,
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
            acceptance: Some("Accepted".into()),
            transaction: Some(tx),
        }
    }

    pub fn orphaned(tx: Transaction, block_id: Hash, index: u32) -> Self {
        Self {
            tx_id: tx.tx_id(),
            status: TxStatus::Orphaned,
            block_id: Some(block_id),
            index: Some(index),
            fee: None,
            confirmations: None,
            acceptance: None,
            transaction: Some(tx),
        }
    }

    pub fn with_acceptance(mut self, acceptance: impl Into<String>) -> Self {
        self.acceptance = Some(acceptance.into());
        self
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
    /// Minimum / suggested fee for wallet coin selection.
    fn estimate_fee(&self) -> Result<FeeEstimate, RpcError>;
    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, RpcError>;
    fn get_balance(&self, address: &Address) -> Amount;
    /// Live UTXO set for wallet coin selection.
    fn get_utxos(&self, address: &Address) -> Result<Vec<UtxoEntry>, RpcError>;
    /// Testnet / faucet credit path. Production node backends may reject this.
    fn fund_address(&mut self, address: Address, amount: Amount) -> Result<Amount, RpcError>;
    /// Mining template (header + coinbase txs) for the CPU sidecar / stratum.
    fn get_block_template(&self) -> Result<Block, RpcError>;
    /// Blue-score–anchored RandomX epoch for a template's parents (default 0).
    fn randomx_epoch(&self, parents: &[Hash]) -> u64 {
        let _ = parents;
        0
    }
    /// Admit a mined block after PoW verification (node) or local insert (tests).
    fn submit_block(&mut self, block: Block) -> Result<Hash, RpcError>;

    // --- Trident finality / staking ---
    fn get_finality(&self, block_hash: &Hash) -> Result<Value, RpcError>;
    fn get_finalized_tip(&self) -> Result<Value, RpcError>;
    fn submit_attestation(&mut self, attestation: Value) -> Result<Value, RpcError>;
    fn get_validator_set(&self, asset: &str, epoch: Option<u64>) -> Result<Value, RpcError>;
    fn get_validator(&self, asset: &str, operator: &Address) -> Result<Value, RpcError>;
    fn get_reward_pool(&self, asset: &str) -> Result<Value, RpcError>;
    /// Admit a secp256k1-signed stake tx (bond/delegate/unbond/withdraw). Never mint-like.
    fn submit_stake_tx(&mut self, stake_tx: Value) -> Result<Value, RpcError>;

    // --- Civic governance / community ---
    fn get_constitution(&self) -> Result<Value, RpcError>;
    fn get_governance(&self) -> Result<Value, RpcError>;
    fn list_proposals(&self, limit: usize) -> Result<Value, RpcError>;
    fn get_proposal(&self, id: u64) -> Result<Value, RpcError>;
    fn list_offices(&self) -> Result<Value, RpcError>;
    fn list_forum_topics(&self, limit: usize) -> Result<Value, RpcError>;
    fn submit_proposal(
        &mut self,
        author: Address,
        title: String,
        summary: String,
        kind: ProposalKind,
        slot: u64,
    ) -> Result<Value, RpcError>;
    fn deposit_proposal(&mut self, id: u64, amount: u64) -> Result<Value, RpcError>;
    fn open_proposal_voting(&mut self, id: u64, slot: u64) -> Result<Value, RpcError>;
    fn cast_gov_vote(
        &mut self,
        id: u64,
        voter: Address,
        choice: VoteChoice,
        raw_balance: u64,
        total_supply: u64,
    ) -> Result<Value, RpcError>;
    fn tally_proposal(&mut self, id: u64) -> Result<Value, RpcError>;
    fn enter_proposal_timelock(&mut self, id: u64, slot: u64) -> Result<Value, RpcError>;
    fn execute_proposal(&mut self, id: u64, slot: u64) -> Result<Value, RpcError>;
    fn post_forum_topic(
        &mut self,
        author: Address,
        title: String,
        body: String,
        category: TopicCategory,
        slot: u64,
    ) -> Result<Value, RpcError>;
    fn ack_constitution(&mut self, address: Address, slot: u64) -> Result<Value, RpcError>;
    fn sponsor_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError>;
    fn assent_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError>;
}

/// In-memory ledger used by unit tests and the local faucet scaffold.
#[derive(Debug)]
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
    civic: Mutex<CivicSnapshot>,
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self {
            tips: Vec::new(),
            blocks: HashMap::new(),
            balances: HashMap::new(),
            utxos: HashMap::new(),
            mempool: HashMap::new(),
            tx_index: HashMap::new(),
            template_bits: 0,
            fund_nonce: 0,
            civic: Mutex::new(CivicSnapshot::genesis(10_000)),
        }
    }
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_civic_mut<R>(
        &self,
        f: impl FnOnce(&mut CivicSnapshot) -> Result<R, RpcError>,
    ) -> Result<R, RpcError> {
        let mut g = self
            .civic
            .lock()
            .map_err(|_| RpcError::Internal("civic lock poisoned".into()))?;
        f(&mut g)
    }

    fn with_civic<R>(
        &self,
        f: impl FnOnce(&CivicSnapshot) -> Result<R, RpcError>,
    ) -> Result<R, RpcError> {
        let g = self
            .civic
            .lock()
            .map_err(|_| RpcError::Internal("civic lock poisoned".into()))?;
        f(&g)
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
                    return match self.tip_confirmations(&block_id) {
                        Some(confirmations) => Ok(TxLookup::confirmed(
                            tx.clone(),
                            block_id,
                            index,
                            confirmations,
                        )),
                        None => Ok(TxLookup::orphaned(tx.clone(), block_id, index)),
                    };
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
            network: "dev".into(),
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
            genesis_hash: self.tips.first().map(|h| h.to_hex()),
            chain_id: Some("agora-dev".into()),
            min_relay_fee: 1,
        })
    }

    fn estimate_fee(&self) -> Result<FeeEstimate, RpcError> {
        Ok(FeeEstimate {
            min_relay_fee: 1,
            suggested_fee: 1,
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
        self.balances.get(address).copied().unwrap_or(Amount::ZERO)
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
        let op = OutPoint { tx_id, index: 0 };
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

    fn get_finality(&self, _block_hash: &Hash) -> Result<Value, RpcError> {
        Ok(json!({
            "state": "Proposed",
            "pow_work_met": false,
            "finalized": false,
            "note": "in-memory backend has no finality store",
        }))
    }

    fn get_finalized_tip(&self) -> Result<Value, RpcError> {
        Ok(json!({ "blue_score": null }))
    }

    fn submit_attestation(&mut self, _attestation: Value) -> Result<Value, RpcError> {
        Err(RpcError::Rejected(
            "in-memory backend does not admit attestations".into(),
        ))
    }

    fn get_validator_set(&self, asset: &str, _epoch: Option<u64>) -> Result<Value, RpcError> {
        Ok(json!({
            "asset": asset,
            "validators": [],
            "total_active_stake": 0,
        }))
    }

    fn get_validator(&self, asset: &str, operator: &Address) -> Result<Value, RpcError> {
        Err(RpcError::NotFound(format!(
            "validator {}/{}",
            asset,
            operator.to_bech32()
        )))
    }

    fn get_reward_pool(&self, asset: &str) -> Result<Value, RpcError> {
        Ok(json!({ "asset": asset, "amount": 0 }))
    }

    fn submit_stake_tx(&mut self, _stake_tx: Value) -> Result<Value, RpcError> {
        Err(RpcError::Rejected(
            "in-memory backend does not apply stake txs".into(),
        ))
    }

    fn get_constitution(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            Ok(json!({
                "id": snap.governance.constitution.id,
                "content_hash": snap.governance.constitution.content_hash_hex(),
                "body_markdown": snap.governance.constitution.body_markdown,
            }))
        })
    }

    fn get_governance(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(civic_overview_json(snap)))
    }

    fn list_proposals(&self, limit: usize) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(list_proposals_json(&snap.governance, limit)))
    }

    fn get_proposal(&self, id: u64) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            let p = snap
                .governance
                .proposal(id)
                .ok_or_else(|| RpcError::NotFound(format!("proposal {id}")))?;
            Ok(proposal_json(p))
        })
    }

    fn list_offices(&self) -> Result<Value, RpcError> {
        self.with_civic(|snap| {
            Ok(json!({
                "offices": snap.governance.offices.seats.iter().map(office_json).collect::<Vec<_>>(),
            }))
        })
    }

    fn list_forum_topics(&self, limit: usize) -> Result<Value, RpcError> {
        self.with_civic(|snap| Ok(list_topics_json(&snap.community, limit)))
    }

    fn submit_proposal(
        &mut self,
        author: Address,
        title: String,
        summary: String,
        kind: ProposalKind,
        slot: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap
                .governance
                .submit_proposal(author, title, summary, kind, slot)
                .map_err(map_gov)?;
            Ok(json!({ "proposal_id": id }))
        })
    }

    fn deposit_proposal(&mut self, id: u64, amount: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance.add_deposit(id, amount).map_err(map_gov)?;
            let p = snap
                .governance
                .proposal(id)
                .ok_or_else(|| RpcError::NotFound(format!("proposal {id}")))?;
            Ok(json!({ "proposal_id": id, "deposit": p.deposit }))
        })
    }

    fn open_proposal_voting(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance.open_voting(id, slot).map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "status": "voting" }))
        })
    }

    fn cast_gov_vote(
        &mut self,
        id: u64,
        voter: Address,
        choice: VoteChoice,
        raw_balance: u64,
        total_supply: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .cast_vote(id, voter, choice, raw_balance, total_supply)
                .map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "voted": true }))
        })
    }

    fn tally_proposal(&mut self, id: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let status = snap.governance.tally(id).map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "status": status }))
        })
    }

    fn enter_proposal_timelock(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance.enter_timelock(id, slot).map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "status": "timelock" }))
        })
    }

    fn execute_proposal(&mut self, id: u64, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance.execute(id, slot).map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "status": "executed" }))
        })
    }

    fn post_forum_topic(
        &mut self,
        author: Address,
        title: String,
        body: String,
        category: TopicCategory,
        slot: u64,
    ) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap
                .community
                .post_topic(author, title, body, category, slot)
                .map_err(map_gov)?;
            Ok(json!({ "topic_id": id }))
        })
    }

    fn ack_constitution(&mut self, address: Address, slot: u64) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            let id = snap.governance.constitution.id.clone();
            let hash = snap.governance.constitution.content_hash_hex();
            snap.community
                .acknowledge_constitution(address, id.clone(), hash.clone(), slot);
            Ok(json!({
                "address": address.to_bech32(),
                "constitution_id": id,
                "constitution_hash": hash,
                "acked": true,
            }))
        })
    }

    fn sponsor_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .sponsor_as_tamias(id, who)
                .map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "sponsored": true }))
        })
    }

    fn assent_proposal(&mut self, id: u64, who: Address) -> Result<Value, RpcError> {
        self.with_civic_mut(|snap| {
            snap.governance
                .record_archon_assent(id, who)
                .map_err(map_gov)?;
            Ok(json!({ "proposal_id": id, "assented": true }))
        })
    }
}

fn map_gov(err: agora_governance::GovernanceError) -> RpcError {
    RpcError::Rejected(err.to_string())
}
