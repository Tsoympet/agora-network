use std::collections::{HashMap, HashSet};

use agora_types::{
    AccountTransfer, Address, Block, Hash, NativeAssetId, OutPoint, SignedStakeTx, Transaction,
};

use crate::P2pError;

/// Default cap on how many transfer txs a mining template pulls from the pool.
pub const DEFAULT_TEMPLATE_TX_LIMIT: usize = 128;

/// Default minimum implicit fee (base units) for relay / template admission.
pub const DEFAULT_MIN_RELAY_FEE: u64 = 1;

/// Local mempool with signature-gated admission and outpoint reservation.
#[derive(Debug, Default)]
pub struct Mempool {
    txs: HashMap<Hash, Transaction>,
    /// Implicit fee (`in − out`) recorded at admit for fee-ordered selection.
    fees: HashMap<Hash, u64>,
    /// Outpoints spent by txs currently in the pool (conflict detection).
    reserved: HashSet<OutPoint>,
    account_txs: HashMap<Hash, AccountTransfer>,
    stake_txs: HashMap<Hash, SignedStakeTx>,
    /// Account and stake lanes share the same per-asset account nonce.
    reserved_accounts: HashSet<(NativeAssetId, Address)>,
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::new(),
            fees: HashMap::new(),
            reserved: HashSet::new(),
            account_txs: HashMap::new(),
            stake_txs: HashMap::new(),
            reserved_accounts: HashSet::new(),
            max_size,
        }
    }

    pub fn len(&self) -> usize {
        self.txs.len() + self.account_txs.len() + self.stake_txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, tx_id: &Hash) -> bool {
        self.txs.contains_key(tx_id)
            || self.account_txs.contains_key(tx_id)
            || self.stake_txs.contains_key(tx_id)
    }

    /// Outpoints already claimed by mempool transactions.
    pub fn reserved(&self) -> &HashSet<OutPoint> {
        &self.reserved
    }

    pub fn account_reserved(&self, asset: NativeAssetId, address: &Address) -> bool {
        self.reserved_accounts.contains(&(asset, *address))
    }

    /// Admit a transaction after secp256k1 verification and mempool conflict checks.
    ///
    /// Prefer [`Self::admit_priced`] when the caller already computed the implicit fee
    /// via [`agora_state_machine::validate_mempool_tx`].
    ///
    /// Callers that have a live UTXO set should run
    /// [`agora_state_machine::validate_mempool_tx`] under the same mempool lock
    /// before this method so chain UTXO rules and reserved conflicts stay atomic.
    pub fn admit(&mut self, tx: Transaction) -> Result<Hash, P2pError> {
        self.admit_priced(tx, 0)
    }

    /// Admit with an explicit fee used for mining-template ordering.
    ///
    /// When the pool is full, lower-fee transactions are evicted (Bitcoin-class
    /// fee market) so a higher-fee tx can enter. Admission fails only if every
    /// resident pays at least as much as the newcomer.
    pub fn admit_priced(&mut self, tx: Transaction, fee: u64) -> Result<Hash, P2pError> {
        if tx.inputs.is_empty() {
            return Err(P2pError::MempoolRejected(
                "coinbase not allowed in mempool".into(),
            ));
        }
        // Callers must verify signatures (preferably network-bound) before admit.
        // Structural auth presence only — domain verify belongs to the UTXO layer.
        if tx.public_key.len() != 33 || tx.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "transaction missing secp256k1 auth".into(),
            ));
        }
        let id = tx.tx_id();
        if self.txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size && !self.evict_lowest_below(fee) {
            return Err(P2pError::MempoolRejected(
                "mempool full; fee too low to evict".into(),
            ));
        }

        let mut claimed = HashSet::new();
        for input in &tx.inputs {
            let op = input.previous_outpoint;
            if !claimed.insert(op) || self.reserved.contains(&op) {
                return Err(P2pError::MempoolRejected(format!(
                    "double spend {}:{}",
                    op.tx_id.to_hex(),
                    op.index
                )));
            }
        }
        for op in &claimed {
            self.reserved.insert(*op);
        }
        self.fees.insert(id, fee);
        self.txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated OVL/DRC account transfer.
    pub fn admit_account(&mut self, tx: AccountTransfer) -> Result<Hash, P2pError> {
        let id = tx.transfer_id();
        if self.account_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (tx.asset, tx.from);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        self.account_txs.insert(id, tx);
        Ok(id)
    }

    /// Admit a pre-validated OVL/DRC stake operation.
    pub fn admit_stake(&mut self, tx: SignedStakeTx) -> Result<Hash, P2pError> {
        let id = tx.stake_tx_id();
        if self.stake_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let key = (tx.asset, tx.actor);
        if !self.reserved_accounts.insert(key) {
            return Err(P2pError::MempoolRejected(
                "account already has a pending nonce".into(),
            ));
        }
        self.stake_txs.insert(id, tx);
        Ok(id)
    }

    /// Drop the lowest-fee resident if its fee is strictly below `fee`.
    /// Returns true when space was made.
    fn evict_lowest_below(&mut self, fee: u64) -> bool {
        let victim = self
            .fees
            .iter()
            .min_by(|(a, fa), (b, fb)| fa.cmp(fb).then_with(|| a.as_bytes().cmp(b.as_bytes())))
            .map(|(id, f)| (*id, *f));
        match victim {
            Some((id, low)) if low < fee => {
                let _ = self.remove(&id);
                true
            }
            _ => false,
        }
    }

    /// Minimum fee currently in the pool (None if empty).
    pub fn min_fee(&self) -> Option<u64> {
        self.fees.values().copied().min()
    }

    /// Median fee of pending txs (None if empty). Used by fee estimation.
    pub fn median_fee(&self) -> Option<u64> {
        if self.fees.is_empty() {
            return None;
        }
        let mut vals: Vec<u64> = self.fees.values().copied().collect();
        vals.sort_unstable();
        Some(vals[vals.len() / 2])
    }

    pub fn get(&self, tx_id: &Hash) -> Option<&Transaction> {
        self.txs.get(tx_id)
    }

    pub fn fee_of(&self, tx_id: &Hash) -> Option<u64> {
        self.fees.get(tx_id).copied()
    }

    /// Lookup by first 8 bytes of `tx_id` for compact-block inflation.
    pub fn get_by_short_id(&self, short_id: &[u8; 8]) -> Option<&Transaction> {
        self.txs
            .iter()
            .find(|(id, _)| &id.as_bytes()[..8] == short_id.as_slice())
            .map(|(_, tx)| tx)
    }

    pub fn remove(&mut self, tx_id: &Hash) -> Option<Transaction> {
        let tx = self.txs.remove(tx_id)?;
        self.fees.remove(tx_id);
        for input in &tx.inputs {
            self.reserved.remove(&input.previous_outpoint);
        }
        Some(tx)
    }

    /// Fee-ordered pending entries (`fee` desc, then `tx_id`) for RPC / templates.
    pub fn pending_entries(&self, max: usize) -> Vec<(Transaction, u64)> {
        let mut entries: Vec<(Transaction, u64)> = self
            .txs
            .values()
            .map(|tx| {
                let fee = self.fees.get(&tx.tx_id()).copied().unwrap_or(0);
                (tx.clone(), fee)
            })
            .collect();
        entries.sort_by(|(a, fa), (b, fb)| {
            fb.cmp(fa)
                .then_with(|| a.tx_id().as_bytes().cmp(b.tx_id().as_bytes()))
        });
        if entries.len() > max {
            entries.truncate(max);
        }
        entries
    }

    /// Fee-ordered transfer selection for mining templates (fee desc, then `tx_id`).
    pub fn select_transfers(&self, max: usize) -> Vec<Transaction> {
        self.pending_entries(max)
            .into_iter()
            .map(|(tx, _)| tx)
            .collect()
    }

    pub fn select_account_transfers(&self, max: usize) -> Vec<AccountTransfer> {
        let mut txs: Vec<_> = self.account_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            b.fee
                .as_base_units()
                .cmp(&a.fee.as_base_units())
                .then_with(|| a.transfer_id().as_bytes().cmp(b.transfer_id().as_bytes()))
        });
        txs.truncate(max);
        txs
    }

    pub fn select_stake_ops(&self, max: usize) -> Vec<SignedStakeTx> {
        let mut txs: Vec<_> = self.stake_txs.values().cloned().collect();
        txs.sort_by_key(SignedStakeTx::stake_tx_id);
        txs.truncate(max);
        txs
    }

    /// Drop included txs and any remaining pool txs that spend the same outpoints.
    pub fn evict_for_block(&mut self, block: &Block) {
        let mut spent = HashSet::new();
        let mut included = HashSet::new();
        for tx in &block.transactions {
            included.insert(tx.tx_id());
            for input in &tx.inputs {
                spent.insert(input.previous_outpoint);
            }
        }
        for tx in &block.account_transfers {
            let id = tx.transfer_id();
            if self.account_txs.remove(&id).is_some() {
                self.reserved_accounts.remove(&(tx.asset, tx.from));
            }
        }
        for tx in &block.stake_ops {
            let id = tx.stake_tx_id();
            if self.stake_txs.remove(&id).is_some() {
                self.reserved_accounts.remove(&(tx.asset, tx.actor));
            }
        }
        let drop: Vec<Hash> = self
            .txs
            .iter()
            .filter(|(id, tx)| {
                included.contains(id)
                    || tx
                        .inputs
                        .iter()
                        .any(|i| spent.contains(&i.previous_outpoint))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in drop {
            let _ = self.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Amount, Hash, OutPoint, Transaction, TxIn, TxOut};

    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn signed_spend(index: u32, nonce: u64) -> Transaction {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            nonce,
        );
        sign_transaction(&mut tx, &kp).unwrap();
        tx
    }

    #[test]
    fn admits_valid_signed_tx() {
        let tx = signed_spend(0, 1);
        let mut pool = Mempool::new(16);
        let id = pool.admit(tx.clone()).unwrap();
        assert_eq!(id, tx.tx_id());
        assert!(pool.contains(&id));
        assert!(pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }

    #[test]
    fn pending_entries_fee_ordered() {
        let low = signed_spend(0, 1);
        let high = signed_spend(1, 2);
        let mut pool = Mempool::new(16);
        pool.admit_priced(low.clone(), 1).unwrap();
        pool.admit_priced(high.clone(), 10).unwrap();
        let entries = pool.pending_entries(16);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.tx_id(), high.tx_id());
        assert_eq!(entries[0].1, 10);
        assert_eq!(entries[1].0.tx_id(), low.tx_id());
    }

    #[test]
    fn rejects_unsigned_tx() {
        let tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![],
            1,
        );
        let mut pool = Mempool::new(16);
        assert!(pool.admit(tx).is_err());
    }

    #[test]
    fn rejects_coinbase_shaped_tx() {
        let tx = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: agora_types::Address::ZERO,
            }],
            1,
        );
        let mut pool = Mempool::new(16);
        let err = pool.admit(tx).unwrap_err().to_string();
        assert!(err.contains("coinbase"), "{err}");
    }

    #[test]
    fn rejects_mempool_double_spend_and_frees_on_remove() {
        let tx_a = signed_spend(0, 1);
        let tx_b = signed_spend(0, 2);
        let mut pool = Mempool::new(16);
        let id_a = pool.admit(tx_a).unwrap();
        assert!(pool.admit(tx_b).is_err());
        pool.remove(&id_a).unwrap();
        assert!(pool.reserved().is_empty());
        let tx_c = signed_spend(0, 3);
        assert!(pool.admit(tx_c).is_ok());
    }

    #[test]
    fn select_transfers_orders_by_fee_then_txid() {
        let mut pool = Mempool::new(16);
        let low = signed_spend(0, 1);
        let high = signed_spend(1, 2);
        let mid = signed_spend(2, 3);
        pool.admit_priced(low.clone(), 1).unwrap();
        pool.admit_priced(high.clone(), 10).unwrap();
        pool.admit_priced(mid.clone(), 5).unwrap();
        let selected = pool.select_transfers(3);
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].tx_id(), high.tx_id());
        assert_eq!(selected[1].tx_id(), mid.tx_id());
        assert_eq!(selected[2].tx_id(), low.tx_id());
        let capped = pool.select_transfers(2);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0].tx_id(), high.tx_id());
    }

    #[test]
    fn full_pool_evicts_lower_fee_for_higher() {
        let mut pool = Mempool::new(2);
        let low = signed_spend(0, 1);
        let mid = signed_spend(1, 2);
        let high = signed_spend(2, 3);
        pool.admit_priced(low.clone(), 1).unwrap();
        pool.admit_priced(mid.clone(), 5).unwrap();
        assert_eq!(pool.len(), 2);
        // Fee not strictly above the lowest resident cannot enter a full pool.
        assert!(pool.admit_priced(high.clone(), 1).is_err());
        // Strictly higher fee evicts the lowest resident.
        let id = pool.admit_priced(high.clone(), 10).unwrap();
        assert_eq!(id, high.tx_id());
        assert!(!pool.contains(&low.tx_id()));
        assert!(pool.contains(&mid.tx_id()));
        assert!(pool.contains(&high.tx_id()));
        assert_eq!(pool.median_fee(), Some(10));
    }

    #[test]
    fn evict_for_block_drops_included_and_conflicts() {
        use agora_types::{Block, BlockHeader};

        let mut pool = Mempool::new(16);
        let included = signed_spend(0, 1);
        let other = signed_spend(1, 2);
        pool.admit(included.clone()).unwrap();
        pool.admit(other.clone()).unwrap();
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![included.clone()],
            account_transfers: vec![],
            stake_ops: vec![],
        };
        pool.evict_for_block(&block);
        assert!(!pool.contains(&included.tx_id()));
        assert!(pool.contains(&other.tx_id()));
        assert!(!pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }
}
