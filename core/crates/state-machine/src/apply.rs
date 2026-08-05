//! Apply / revert consensus-ordered blocks against the UTXO set.

use std::collections::{HashMap, HashSet};

use agora_crypto::{address_from_pubkey, signer_address, verify_transaction_bound, PublicKeyBytes};
use agora_types::{Amount, Block, Hash, OutPoint, Transaction, TxOut};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::utxo::outpoint_key;
use crate::{StateError, StateStore};

/// Network domain for transaction signatures (`chain_id` + genesis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxAuthContext {
    pub chain_id: String,
    pub genesis: Hash,
}

/// Journal of UTXO mutations so a failed admission / reorg can roll back.
///
/// Accounting fields (`fees`, `subsidy`, `coinbase_total`) are persisted so unapply
/// never reverse-engineers fees from spent/created lists (which omit same-block
/// parent→child package edges).
#[derive(Debug, Default, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UtxoJournal {
    /// Outputs removed while applying (for revert: re-insert).
    pub spent: Vec<(OutPoint, TxOut)>,
    /// Outpoints created while applying (for revert: delete).
    pub created: Vec<OutPoint>,
    /// Exact transfer fee total (`Σ in − out`) for this block.
    pub fees: u64,
    /// Exact coinbase subsidy (`coinbase_total − fees`) credited to issued supply.
    pub subsidy: u64,
    /// Sum of coinbase outputs.
    pub coinbase_total: u64,
}

/// Pre-v2 journal (spent + created only) for load migration.
#[derive(Debug, Clone, BorshDeserialize)]
struct LegacyUtxoJournal {
    spent: Vec<(OutPoint, TxOut)>,
    created: Vec<OutPoint>,
}

impl UtxoJournal {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StateError> {
        if let Ok(j) = Self::try_from_slice(bytes) {
            return Ok(j);
        }
        let legacy = LegacyUtxoJournal::try_from_slice(bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        Ok(Self {
            spent: legacy.spent,
            created: legacy.created,
            fees: 0,
            subsidy: 0,
            coinbase_total: 0,
        })
    }
}

/// Implicit fee of a transfer (`input − output`) against the live UTXO set.
pub fn transfer_fee(store: &StateStore, tx: &Transaction) -> Result<u64, StateError> {
    if tx.inputs.is_empty() {
        return Ok(0);
    }
    let mut input_value = 0u64;
    for input in &tx.inputs {
        let out = load_utxo(store, &input.previous_outpoint)?;
        input_value = input_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("input value overflow".into()))?;
    }
    let mut output_value = 0u64;
    for out in &tx.outputs {
        output_value = output_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("output value overflow".into()))?;
    }
    if input_value < output_value {
        return Err(StateError::InvalidTx(format!(
            "insufficient funds: in={input_value} out={output_value}"
        )));
    }
    Ok(input_value - output_value)
}

/// Sum of transfer fees in `txs` (coinbase-shaped entries contribute 0).
///
/// Package-aware: an input that spends an output created by an **earlier** transaction in
/// the same `txs` list is valued from that in-block output, so parent→child transaction
/// packages are priced consistently with [`apply_block`] (which spends same-block outputs
/// via `created_in_block`). Falls back to the live UTXO set for pre-existing inputs.
pub fn sum_transfer_fees(store: &StateStore, txs: &[Transaction]) -> Result<u64, StateError> {
    let mut total = 0u64;
    let mut created: HashMap<OutPoint, TxOut> = HashMap::new();
    for tx in txs {
        if !tx.inputs.is_empty() {
            total = total
                .checked_add(transfer_fee_in_context(store, tx, &created)?)
                .ok_or_else(|| StateError::InvalidTx("fee overflow".into()))?;
        }
        let tx_id = tx.tx_id();
        for (index, out) in tx.outputs.iter().enumerate() {
            created.insert(
                OutPoint {
                    tx_id,
                    index: index as u32,
                },
                out.clone(),
            );
        }
    }
    Ok(total)
}

/// Implicit fee of a transfer, resolving inputs against `created` (same-block outputs)
/// first, then the live UTXO set.
fn transfer_fee_in_context(
    store: &StateStore,
    tx: &Transaction,
    created: &HashMap<OutPoint, TxOut>,
) -> Result<u64, StateError> {
    if tx.inputs.is_empty() {
        return Ok(0);
    }
    let mut input_value = 0u64;
    for input in &tx.inputs {
        let out = match created.get(&input.previous_outpoint) {
            Some(out) => out.clone(),
            None => load_utxo(store, &input.previous_outpoint)?,
        };
        input_value = input_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("input value overflow".into()))?;
    }
    let mut output_value = 0u64;
    for out in &tx.outputs {
        output_value = output_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("output value overflow".into()))?;
    }
    if input_value < output_value {
        return Err(StateError::InvalidTx(format!(
            "insufficient funds: in={input_value} out={output_value}"
        )));
    }
    Ok(input_value - output_value)
}

/// Apply all transactions in `block` to `cf_utxo`.
///
/// Rules:
/// - **Exactly one** coinbase (`inputs` empty); its outputs must total
///   ≤ `emission_reward + Σ transfer fees`
/// - Non-coinbase txs must verify secp256k1 auth; each spent UTXO must belong to the signer
/// - Input value ≥ output value (difference is the fee paid to the coinbase miner)
/// - No double-spends within the block or against the live set
/// - Newly created outpoints must not already exist (duplicate coinbase bodies rejected)
pub fn apply_block(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
) -> Result<UtxoJournal, StateError> {
    apply_block_with_auth(store, block, emission_reward, None)
}

/// Like [`apply_block`] with optional network-bound signature verification.
pub fn apply_block_with_auth(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
    auth: Option<&TxAuthContext>,
) -> Result<UtxoJournal, StateError> {
    let (journal, batch) = apply_block_batched_with_auth(store, block, emission_reward, auth)?;
    store.write_batch(batch)?;
    Ok(journal)
}

/// Compute a block's UTXO transition without mutating the store.
///
/// Returns the revert [`UtxoJournal`] and an uncommitted [`WriteBatch`] of the UTXO
/// changes. Callers can extend the batch (e.g. with the journal record and issued-supply
/// update) and commit everything atomically via [`StateStore::write_batch`].
pub fn apply_block_batched(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
) -> Result<(UtxoJournal, WriteBatch), StateError> {
    apply_block_batched_with_auth(store, block, emission_reward, None)
}

/// Like [`apply_block_batched`] with optional network-bound signature verification.
pub fn apply_block_batched_with_auth(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
    auth: Option<&TxAuthContext>,
) -> Result<(UtxoJournal, WriteBatch), StateError> {
    let fees = sum_transfer_fees(store, &block.transactions)?;
    let coinbase_budget = emission_reward
        .checked_add(fees)
        .ok_or_else(|| StateError::Coinbase("reward overflow".into()))?;

    let mut batch = WriteBatch::new();
    let mut journal = UtxoJournal::default();
    let mut spent_in_block: HashSet<OutPoint> = HashSet::new();
    let mut created_in_block: HashMap<OutPoint, TxOut> = HashMap::new();
    let mut coinbase_created: HashSet<OutPoint> = HashSet::new();
    let mut coinbases = 0u32;
    let mut coinbase_total = 0u64;

    for tx in &block.transactions {
        if tx.inputs.is_empty() {
            coinbases += 1;
            if coinbases > 1 {
                return Err(StateError::Coinbase("multiple coinbase txs".into()));
            }
            coinbase_total = apply_coinbase(
                store,
                tx,
                coinbase_budget,
                &mut batch,
                &mut journal,
                &mut created_in_block,
                &mut coinbase_created,
            )?;
        } else {
            apply_transfer(
                store,
                tx,
                auth,
                &mut batch,
                &mut journal,
                &mut spent_in_block,
                &mut created_in_block,
                &coinbase_created,
            )?;
        }
    }
    if coinbases == 0 {
        return Err(StateError::Coinbase("missing coinbase".into()));
    }

    journal.fees = fees;
    journal.coinbase_total = coinbase_total;
    journal.subsidy = coinbase_total.saturating_sub(fees);

    Ok((journal, batch))
}

fn apply_coinbase(
    store: &StateStore,
    tx: &Transaction,
    coinbase_reward: u64,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
    coinbase_created: &mut HashSet<OutPoint>,
) -> Result<u64, StateError> {
    if tx.outputs.is_empty() {
        return Err(StateError::Coinbase("coinbase has no outputs".into()));
    }
    let mut total = 0u64;
    for out in &tx.outputs {
        total = total
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::Coinbase("output overflow".into()))?;
    }
    if total > coinbase_reward {
        return Err(StateError::Coinbase(format!(
            "coinbase {total} exceeds reward {coinbase_reward}"
        )));
    }
    create_outputs(
        store,
        batch,
        tx,
        journal,
        created_in_block,
        Some(coinbase_created),
    )?;
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn apply_transfer(
    store: &StateStore,
    tx: &Transaction,
    auth: Option<&TxAuthContext>,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
    spent_in_block: &mut HashSet<OutPoint>,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
    coinbase_created: &HashSet<OutPoint>,
) -> Result<(), StateError> {
    let signer = verify_and_signer(tx, auth)?;

    let mut input_value = 0u64;
    let mut pending_spends: Vec<(OutPoint, TxOut)> = Vec::new();

    for input in &tx.inputs {
        let op = input.previous_outpoint;
        if spent_in_block.contains(&op) {
            return Err(StateError::DoubleSpend(format!(
                "{}:{}",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        if coinbase_created.contains(&op) {
            return Err(StateError::ImmatureCoinbase(format!(
                "{}:{} (same-block)",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        let out = if let Some(created) = created_in_block.get(&op) {
            created.clone()
        } else {
            load_utxo(store, &op)?
        };
        if out.address != signer {
            return Err(StateError::InvalidTx(format!(
                "input {}:{} not owned by signer",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        input_value = input_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("input value overflow".into()))?;
        pending_spends.push((op, out));
    }

    let mut output_value = 0u64;
    for out in &tx.outputs {
        output_value = output_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("output value overflow".into()))?;
    }
    if input_value < output_value {
        return Err(StateError::InvalidTx(format!(
            "insufficient funds: in={input_value} out={output_value}"
        )));
    }

    for (op, out) in pending_spends {
        spend_utxo(batch, &op, &out, journal, spent_in_block, created_in_block);
    }
    create_outputs(store, batch, tx, journal, created_in_block, None)
}

fn verify_and_signer(
    tx: &Transaction,
    auth: Option<&TxAuthContext>,
) -> Result<agora_types::Address, StateError> {
    match auth {
        Some(ctx) => {
            verify_transaction_bound(tx, &ctx.chain_id, &ctx.genesis)
                .map_err(|e| StateError::InvalidTx(e.to_string()))?;
            if tx.public_key.len() != 33 {
                return Err(StateError::InvalidTx("bad pubkey".into()));
            }
            let mut pubkey: PublicKeyBytes = [0u8; 33];
            pubkey.copy_from_slice(&tx.public_key);
            Ok(address_from_pubkey(&pubkey))
        }
        None => signer_address(tx).map_err(|e| StateError::InvalidTx(e.to_string())),
    }
}

fn load_utxo(store: &StateStore, op: &OutPoint) -> Result<TxOut, StateError> {
    let key = outpoint_key(op);
    let bytes = store
        .get_cf(ColumnFamily::Utxo, &key)?
        .ok_or_else(|| StateError::MissingUtxo(format!("{}:{}", op.tx_id.to_hex(), op.index)))?;
    TxOut::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

fn spend_utxo(
    batch: &mut WriteBatch,
    op: &OutPoint,
    out: &TxOut,
    journal: &mut UtxoJournal,
    spent_in_block: &mut HashSet<OutPoint>,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
) {
    let key = outpoint_key(op);
    batch.delete_cf(ColumnFamily::Utxo, &key);
    if created_in_block.remove(op).is_some() {
        // Created earlier in this same block — drop from journal so revert is a no-op.
        journal.created.retain(|c| c != op);
    } else {
        journal.spent.push((*op, out.clone()));
    }
    spent_in_block.insert(*op);
}

fn create_outputs(
    store: &StateStore,
    batch: &mut WriteBatch,
    tx: &Transaction,
    journal: &mut UtxoJournal,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
    mut coinbase_created: Option<&mut HashSet<OutPoint>>,
) -> Result<(), StateError> {
    let tx_id = tx.tx_id();
    for (index, out) in tx.outputs.iter().enumerate() {
        let op = OutPoint {
            tx_id,
            index: index as u32,
        };
        if created_in_block.contains_key(&op) {
            return Err(StateError::DuplicateOutpoint(format!(
                "{}:{}",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        let key = outpoint_key(&op);
        if store.get_cf(ColumnFamily::Utxo, &key)?.is_some() {
            return Err(StateError::DuplicateOutpoint(format!(
                "{}:{} (already in utxo set)",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        let bytes = borsh::to_vec(out).map_err(|e| StateError::Storage(e.to_string()))?;
        batch.put_cf(ColumnFamily::Utxo, &key, &bytes);
        journal.created.push(op);
        created_in_block.insert(op, out.clone());
        if let Some(set) = coinbase_created.as_deref_mut() {
            set.insert(op);
        }
    }
    Ok(())
}

/// Reverse a successful [`apply_block`] journal (best-effort atomicity for scaffolds).
pub fn revert_journal(store: &StateStore, journal: &UtxoJournal) -> Result<(), StateError> {
    let batch = revert_journal_batched(journal)?;
    store.write_batch(batch)
}

/// Build an uncommitted [`WriteBatch`] that reverses `journal`.
///
/// Callers can fold multiple unapply/apply journals into one batch with supply and
/// tip meta for a crash-safe multi-block reorg.
pub fn revert_journal_batched(journal: &UtxoJournal) -> Result<WriteBatch, StateError> {
    let mut batch = WriteBatch::new();
    for op in journal.created.iter().rev() {
        batch.delete_cf(ColumnFamily::Utxo, &outpoint_key(op));
    }
    for (op, out) in journal.spent.iter().rev() {
        let bytes = borsh::to_vec(out).map_err(|e| StateError::Storage(e.to_string()))?;
        batch.put_cf(ColumnFamily::Utxo, &outpoint_key(op), &bytes);
    }
    Ok(batch)
}

/// Read-only UTXO checks for mempool / gossip admission (does not mutate `cf_utxo`).
///
/// Rejects coinbase-shaped txs (`inputs` empty), missing / foreign / already-reserved
/// outpoints, and transfers whose outputs exceed input value.
/// Validate a mempool candidate against the live UTXO set.
///
/// Returns the implicit fee (`input − output`) on success.
pub fn validate_mempool_tx(
    store: &StateStore,
    tx: &Transaction,
    reserved: &HashSet<OutPoint>,
) -> Result<u64, StateError> {
    validate_mempool_tx_with_auth(store, tx, reserved, None)
}

/// Mempool validation with optional network-bound signatures.
pub fn validate_mempool_tx_with_auth(
    store: &StateStore,
    tx: &Transaction,
    reserved: &HashSet<OutPoint>,
    auth: Option<&TxAuthContext>,
) -> Result<u64, StateError> {
    if tx.inputs.is_empty() {
        return Err(StateError::InvalidTx(
            "coinbase not allowed in mempool".into(),
        ));
    }
    if tx.inputs.len() > agora_consensus::MAX_TX_INPUTS {
        return Err(StateError::BlockLimit(format!(
            "too many inputs: {} > {}",
            tx.inputs.len(),
            agora_consensus::MAX_TX_INPUTS
        )));
    }
    if tx.outputs.len() > agora_consensus::MAX_TX_OUTPUTS {
        return Err(StateError::BlockLimit(format!(
            "too many outputs: {} > {}",
            tx.outputs.len(),
            agora_consensus::MAX_TX_OUTPUTS
        )));
    }
    let tx_bytes = borsh::to_vec(tx).map_err(|e| StateError::Storage(e.to_string()))?;
    if tx_bytes.len() > agora_consensus::MAX_TX_BYTES {
        return Err(StateError::BlockLimit(format!(
            "tx too large: {} > {}",
            tx_bytes.len(),
            agora_consensus::MAX_TX_BYTES
        )));
    }
    let signer = verify_and_signer(tx, auth)?;

    let mut input_value = 0u64;
    let mut seen: HashSet<OutPoint> = HashSet::new();
    for input in &tx.inputs {
        let op = input.previous_outpoint;
        if !seen.insert(op) || reserved.contains(&op) {
            return Err(StateError::DoubleSpend(format!(
                "{}:{}",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        let out = load_utxo(store, &op)?;
        if out.address != signer {
            return Err(StateError::InvalidTx(format!(
                "input {}:{} not owned by signer",
                op.tx_id.to_hex(),
                op.index
            )));
        }
        input_value = input_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("input value overflow".into()))?;
    }

    let mut output_value = 0u64;
    for out in &tx.outputs {
        output_value = output_value
            .checked_add(out.value.as_base_units())
            .ok_or_else(|| StateError::InvalidTx("output value overflow".into()))?;
    }
    if input_value < output_value {
        return Err(StateError::InvalidTx(format!(
            "insufficient funds: in={input_value} out={output_value}"
        )));
    }
    Ok(input_value - output_value)
}

/// Sum of all UTXO values for `address` (same scan used by RPC balances).
pub fn balance_of(
    store: &StateStore,
    address: &agora_types::Address,
) -> Result<Amount, StateError> {
    let mut total = Amount::ZERO;
    store.for_each_cf(ColumnFamily::Utxo, |_key, value| {
        let out = TxOut::try_from_slice(value).map_err(|e| StateError::Storage(e.to_string()))?;
        if &out.address == address {
            total = total
                .checked_add(out.value)
                .ok_or_else(|| StateError::Storage("balance overflow".into()))?;
        }
        Ok(())
    })?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{
        Address, Amount, Block, BlockHeader, Hash, OutPoint, Transaction, TxIn, TxOut,
    };

    use super::*;
    use crate::genesis::GenesisBuilder;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn apply_transfer_spends_premine_and_reverts() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();

        let genesis_hash = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis_hash.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis.transactions[0].tx_id();
        let premine = Amount::from_whole(10_000_000).unwrap();

        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![
                TxOut {
                    value: Amount::from_whole(1).unwrap(),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(
                        premine.as_base_units() - Amount::from_whole(1).unwrap().as_base_units(),
                    ),
                    address: from.address(),
                },
            ],
            1,
        );
        sign_transaction(&mut tx, &from).unwrap();

        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::ZERO,
                address: Address::ZERO,
            }],
            1,
        );
        let txs = vec![coinbase, tx.clone()];
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_hash],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(&txs),
            },
            transactions: txs,
        };

        let journal = apply_block(&store, &block, 0).unwrap();
        assert_eq!(journal.fees, 0);
        assert_eq!(journal.subsidy, 0);
        assert_eq!(
            balance_of(&store, &to).unwrap().as_base_units(),
            Amount::from_whole(1).unwrap().as_base_units()
        );
        assert!(load_utxo(
            &store,
            &OutPoint {
                tx_id: premine_txid,
                index: 0
            }
        )
        .is_err());

        revert_journal(&store, &journal).unwrap();
        assert_eq!(balance_of(&store, &to).unwrap(), Amount::ZERO);
        assert_eq!(
            balance_of(&store, &from.address()).unwrap().as_base_units(),
            premine.as_base_units()
        );
        let _ = Address::ZERO;
        let _ = Hash::ZERO;
    }

    #[test]
    fn validate_mempool_tx_accepts_premine_spend() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let genesis_hash = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis_hash.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis.transactions[0].tx_id();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_whole(1).unwrap(),
                address: to,
            }],
            1,
        );
        sign_transaction(&mut tx, &from).unwrap();
        validate_mempool_tx(&store, &tx, &HashSet::new()).unwrap();

        let mut reserved = HashSet::new();
        reserved.insert(OutPoint {
            tx_id: premine_txid,
            index: 0,
        });
        assert!(matches!(
            validate_mempool_tx(&store, &tx, &reserved),
            Err(StateError::DoubleSpend(_))
        ));
    }

    #[test]
    fn validate_mempool_tx_rejects_coinbase_and_missing_utxo() {
        let store = StateStore::open_in_memory();
        GenesisBuilder::default().ignite(&store).unwrap();
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: Address::ZERO,
            }],
            0,
        );
        assert!(matches!(
            validate_mempool_tx(&store, &coinbase, &HashSet::new()),
            Err(StateError::InvalidTx(_))
        ));

        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let mut missing = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            1,
        );
        sign_transaction(&mut missing, &kp).unwrap();
        assert!(matches!(
            validate_mempool_tx(&store, &missing, &HashSet::new()),
            Err(StateError::MissingUtxo(_))
        ));
    }

    #[test]
    fn coinbase_may_claim_emission_plus_transfer_fees() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let miner = Address([3u8; 20]);
        let genesis_hash = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis_hash.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis.transactions[0].tx_id();
        let premine = Amount::from_whole(10_000_000).unwrap().as_base_units();
        let pay = Amount::from_whole(1).unwrap().as_base_units();
        let fee = 5u64;
        let mut transfer = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![
                TxOut {
                    value: Amount::from_base_units(pay),
                    address: to,
                },
                TxOut {
                    value: Amount::from_base_units(premine - pay - fee),
                    address: from.address(),
                },
            ],
            9,
        );
        sign_transaction(&mut transfer, &from).unwrap();
        assert_eq!(transfer_fee(&store, &transfer).unwrap(), fee);

        let emission = 100u64;
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(emission + fee),
                address: miner,
            }],
            0,
        );
        let txs = vec![coinbase, transfer];
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_hash],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(&txs),
            },
            transactions: txs,
        };
        apply_block(&store, &block, emission).unwrap();
        assert_eq!(
            balance_of(&store, &miner).unwrap().as_base_units(),
            emission + fee
        );
        assert_eq!(balance_of(&store, &to).unwrap().as_base_units(), pay);
    }

    #[test]
    fn package_fees_account_for_same_block_child() {
        // Parent tx spends premine; child tx spends the parent's change output in the SAME
        // block. Fee pre-calc must value the child's input from the in-block output, not
        // error with MissingUtxo.
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let from = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let genesis_hash = GenesisBuilder::default()
            .with_premine_address(from.address())
            .ignite(&store)
            .unwrap();
        let genesis = {
            let bytes = store
                .get_cf(ColumnFamily::Hot, genesis_hash.as_bytes())
                .unwrap()
                .unwrap();
            Block::try_from_slice(&bytes).unwrap()
        };
        let premine_txid = genesis.transactions[0].tx_id();
        let premine = Amount::from_whole(10_000_000).unwrap().as_base_units();

        // Parent: spend premine, pay 2 back to self (change), fee = premine - 2.
        let parent_change = 200u64;
        let mut parent = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(parent_change),
                address: from.address(),
            }],
            1,
        );
        sign_transaction(&mut parent, &from).unwrap();
        let parent_id = parent.tx_id();

        // Child: spend parent's output 0 (same block), pay 50 to self, fee = 150.
        let mut child = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: parent_id,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(50),
                address: from.address(),
            }],
            2,
        );
        sign_transaction(&mut child, &from).unwrap();

        let txs = vec![parent, child];
        // Package-aware fee sum: parent fee (premine-200) + child fee (150).
        let fees = sum_transfer_fees(&store, &txs).unwrap();
        assert_eq!(fees, (premine - parent_change) + (parent_change - 50));

        // And the block applies end-to-end with a coinbase claiming those fees.
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(fees),
                address: Address([7u8; 20]),
            }],
            0,
        );
        let mut all = vec![coinbase];
        all.extend(txs);
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_hash],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(&all),
            },
            transactions: all,
        };
        apply_block(&store, &block, 0).unwrap();
        assert_eq!(
            balance_of(&store, &Address([7u8; 20]))
                .unwrap()
                .as_base_units(),
            fees
        );
    }

    #[test]
    fn write_batch_is_atomic() {
        let store = StateStore::open_in_memory();
        let mut batch = crate::WriteBatch::new();
        batch.put_cf(ColumnFamily::Meta, b"a", b"1");
        batch.put_cf(ColumnFamily::Meta, b"b", b"2");
        assert_eq!(batch.len(), 2);
        store.write_batch(batch).unwrap();
        assert_eq!(
            store.get_cf(ColumnFamily::Meta, b"a").unwrap(),
            Some(b"1".to_vec())
        );
        assert_eq!(
            store.get_cf(ColumnFamily::Meta, b"b").unwrap(),
            Some(b"2".to_vec())
        );

        let mut batch = crate::WriteBatch::new();
        batch.delete_cf(ColumnFamily::Meta, b"a");
        batch.put_cf(ColumnFamily::Meta, b"b", b"3");
        store.write_batch(batch).unwrap();
        assert_eq!(store.get_cf(ColumnFamily::Meta, b"a").unwrap(), None);
        assert_eq!(
            store.get_cf(ColumnFamily::Meta, b"b").unwrap(),
            Some(b"3".to_vec())
        );
    }

    #[test]
    fn rejects_oversized_coinbase() {
        let store = StateStore::open_in_memory();
        GenesisBuilder::default().ignite(&store).unwrap();
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(100),
                address: Address::ZERO,
            }],
            0,
        );
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(std::slice::from_ref(&coinbase)),
            },
            transactions: vec![coinbase],
        };
        assert!(matches!(
            apply_block(&store, &block, 50),
            Err(StateError::Coinbase(_))
        ));
    }
}
