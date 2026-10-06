//! Block admission for versioned TLT covenant transactions.
//!
//! v1 [`agora_types::Transaction`] bytes and address-locked `TxOut` records stay
//! as they are. Covenant spends are a separate block lane. A P2PKH-style output
//! is stored as a normal address UTXO so premine and v1 wallets can still spend
//! it. Every other script lives under a meta key and is restored from the journal
//! on reorg.

#![allow(clippy::too_many_arguments)]

use std::collections::{HashMap, HashSet};

use agora_crypto::verify_tlt_covenant_input_preimage;
use agora_types::{
    covenant_locktime_satisfied, p2pkh_address, script_p2pkh, Amount, Block, OutPoint,
    TltCovenantTx, TltOutputOrigin, TxOut, TLT_COVENANT_TX_VERSION,
};
use borsh::BorshDeserialize;

use crate::apply::{spend_utxo, UtxoJournal};
use crate::columns::ColumnFamily;
use crate::headers::load_header;
use crate::store::WriteBatch;
use crate::utxo::outpoint_key;
use crate::{ApplyMode, StateError, StateStore, TxAuthContext};

/// Script-locked output. Address-locked P2PKH outputs stay in `cf_utxo` as [`TxOut`].
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TltCovenantUtxoRecord {
    pub value: Amount,
    pub script_pubkey: Vec<u8>,
    pub origin_blue_score: u64,
    pub origin_median_time_secs: u64,
}

impl TltCovenantUtxoRecord {
    fn origin(&self) -> TltOutputOrigin {
        TltOutputOrigin {
            blue_score: self.origin_blue_score,
            median_time_secs: self.origin_median_time_secs,
        }
    }
}

const COVENANT_UTXO_PREFIX: &[u8] = b"tlt/covenant/utxo/";

pub fn covenant_utxo_key(op: &OutPoint) -> Vec<u8> {
    let mut key = Vec::with_capacity(COVENANT_UTXO_PREFIX.len() + 64);
    key.extend_from_slice(COVENANT_UTXO_PREFIX);
    key.extend(borsh::to_vec(op).expect("outpoint borsh"));
    key
}

pub fn load_covenant_utxo(
    store: &StateStore,
    op: &OutPoint,
) -> Result<Option<TltCovenantUtxoRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &covenant_utxo_key(op))? else {
        return Ok(None);
    };
    let record = TltCovenantUtxoRecord::try_from_slice(&bytes)
        .map_err(|err| StateError::Storage(err.to_string()))?;
    Ok(Some(record))
}

/// Median parent header time in unix seconds, or this block's own timestamp.
pub fn block_median_time_secs(store: &StateStore, block: &Block) -> Result<u64, StateError> {
    let mut times = Vec::new();
    for parent in &block.header.parents {
        if let Some(header) = load_header(store, parent)? {
            times.push(header.timestamp_ms / 1000);
        }
    }
    if times.is_empty() {
        return Ok(block.header.timestamp_ms / 1000);
    }
    times.sort_unstable();
    Ok(times[times.len() / 2])
}

#[derive(Clone)]
enum Resolved {
    Address {
        output: TxOut,
        origin: TltOutputOrigin,
    },
    Script {
        record: TltCovenantUtxoRecord,
        from_store: bool,
    },
}

struct SelectError {
    hard: bool,
    error: StateError,
}

/// Fees of covenant transactions that will apply, in lane order.
#[allow(clippy::too_many_arguments)]
pub fn selectable_covenant_fees(
    store: &StateStore,
    block: &Block,
    auth: Option<&TxAuthContext>,
    blue_score: u64,
    median_time_secs: u64,
    mode: ApplyMode,
    address_created: &HashMap<OutPoint, TxOut>,
    v1_spent: &HashSet<OutPoint>,
    coinbase_created: &HashSet<OutPoint>,
) -> Result<(Vec<usize>, u64), StateError> {
    let mut indexes = Vec::new();
    let mut total = 0u64;
    let mut spent = v1_spent.clone();
    let mut scripts: HashMap<OutPoint, TltCovenantUtxoRecord> = HashMap::new();
    let mut seen_ids = HashSet::new();
    for (index, tx) in block.tlt_covenants.iter().enumerate() {
        match inspect_covenant(
            store,
            tx,
            auth,
            blue_score,
            median_time_secs,
            &spent,
            address_created,
            &scripts,
            coinbase_created,
        ) {
            Ok(inspection) => {
                let id = tx.tx_id();
                if !seen_ids.insert(id) {
                    return Err(StateError::InvalidTx(
                        "duplicate covenant transaction id".into(),
                    ));
                }
                for (op, _) in &inspection.spends {
                    spent.insert(*op);
                }
                let tx_id = tx.tx_id();
                for (output_index, record) in inspection.created_scripts {
                    scripts.insert(
                        OutPoint {
                            tx_id,
                            index: output_index,
                        },
                        record,
                    );
                }
                total = total
                    .checked_add(inspection.fee)
                    .ok_or_else(|| StateError::InvalidTx("covenant fee overflow".into()))?;
                indexes.push(index);
            }
            Err(SelectError { hard: true, error }) => return Err(error),
            Err(SelectError { hard: false, .. }) if mode == ApplyMode::Virtual => continue,
            Err(SelectError { error, .. }) => return Err(error),
        }
    }
    Ok((indexes, total))
}

struct Inspection {
    fee: u64,
    spends: Vec<(OutPoint, Resolved)>,
    created_scripts: Vec<(u32, TltCovenantUtxoRecord)>,
}

#[allow(clippy::too_many_arguments)]
fn inspect_covenant(
    store: &StateStore,
    tx: &TltCovenantTx,
    auth: Option<&TxAuthContext>,
    blue_score: u64,
    median_time_secs: u64,
    spent: &HashSet<OutPoint>,
    address_created: &HashMap<OutPoint, TxOut>,
    scripts: &HashMap<OutPoint, TltCovenantUtxoRecord>,
    coinbase_created: &HashSet<OutPoint>,
) -> Result<Inspection, SelectError> {
    structural(tx).map_err(hard)?;
    let mut pending_spent = HashSet::new();
    let mut spends = Vec::new();
    let mut input_value = 0u64;
    for (input_index, input) in tx.inputs.iter().enumerate() {
        let op = input.previous_outpoint;
        if !pending_spent.insert(op) || spent.contains(&op) {
            return Err(soft(StateError::DoubleSpend(format!(
                "{}:{}",
                op.tx_id.to_hex(),
                op.index
            ))));
        }
        if coinbase_created.contains(&op) {
            return Err(hard(StateError::ImmatureCoinbase(format!(
                "{}:{} (same-block)",
                op.tx_id.to_hex(),
                op.index
            ))));
        }
        let resolved = resolve_input(
            store,
            &op,
            address_created,
            scripts,
            blue_score,
            median_time_secs,
        )
        .map_err(|err| match err {
            StateError::MissingUtxo(_) => soft(err),
            other => hard(other),
        })?;
        let script = script_of(&resolved);
        let origin = origin_of(&resolved);
        let preimage = sighash(tx, auth);
        verify_tlt_covenant_input_preimage(
            tx,
            input_index,
            &script,
            blue_score,
            median_time_secs,
            origin,
            &preimage,
        )
        .map_err(|err| hard(StateError::InvalidTx(err.to_string())))?;
        input_value = input_value
            .checked_add(value_of(&resolved))
            .ok_or_else(|| hard(StateError::InvalidTx("input value overflow".into())))?;
        spends.push((op, resolved));
    }
    covenant_locktime_satisfied(tx, blue_score, median_time_secs)
        .map_err(|err| hard(StateError::InvalidTx(err.to_string())))?;
    let mut output_value = 0u64;
    let mut created_scripts = Vec::new();
    for (index, output) in tx.outputs.iter().enumerate() {
        if output.script_pubkey.is_empty() {
            return Err(hard(StateError::InvalidTx("empty covenant script".into())));
        }
        output_value = output_value
            .checked_add(output.value.as_base_units())
            .ok_or_else(|| hard(StateError::InvalidTx("output value overflow".into())))?;
        if p2pkh_address(&output.script_pubkey).is_none() {
            created_scripts.push((
                index as u32,
                TltCovenantUtxoRecord {
                    value: output.value,
                    script_pubkey: output.script_pubkey.clone(),
                    origin_blue_score: blue_score,
                    origin_median_time_secs: median_time_secs,
                },
            ));
        }
    }
    if input_value < output_value {
        return Err(hard(StateError::InvalidTx(format!(
            "insufficient funds: in={input_value} out={output_value}"
        ))));
    }
    Ok(Inspection {
        fee: input_value - output_value,
        spends,
        created_scripts,
    })
}

fn structural(tx: &TltCovenantTx) -> Result<(), StateError> {
    if tx.version != TLT_COVENANT_TX_VERSION {
        return Err(StateError::InvalidTx(format!(
            "covenant version {} is not {TLT_COVENANT_TX_VERSION}",
            tx.version
        )));
    }
    if tx.inputs.is_empty() {
        return Err(StateError::InvalidTx(
            "covenant transaction has no inputs".into(),
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
    let bytes = borsh::to_vec(tx).map_err(|err| StateError::Storage(err.to_string()))?;
    if bytes.len() > agora_consensus::MAX_TX_BYTES {
        return Err(StateError::BlockLimit(format!(
            "tx too large: {} > {}",
            bytes.len(),
            agora_consensus::MAX_TX_BYTES
        )));
    }
    Ok(())
}

fn resolve_input(
    store: &StateStore,
    op: &OutPoint,
    address_created: &HashMap<OutPoint, TxOut>,
    scripts: &HashMap<OutPoint, TltCovenantUtxoRecord>,
    blue_score: u64,
    median_time_secs: u64,
) -> Result<Resolved, StateError> {
    if let Some(output) = address_created.get(op) {
        return Ok(Resolved::Address {
            output: output.clone(),
            origin: TltOutputOrigin {
                blue_score,
                median_time_secs,
            },
        });
    }
    if let Some(record) = scripts.get(op) {
        return Ok(Resolved::Script {
            record: record.clone(),
            from_store: false,
        });
    }
    let address = match store.get_cf(ColumnFamily::Utxo, &outpoint_key(op))? {
        Some(bytes) => Some(
            TxOut::try_from_slice(&bytes).map_err(|err| StateError::Storage(err.to_string()))?,
        ),
        None => None,
    };
    let script = load_covenant_utxo(store, op)?;
    match (address, script) {
        (Some(_), Some(_)) => Err(StateError::Storage(format!(
            "utxo {}:{} is both an address output and a covenant script",
            op.tx_id.to_hex(),
            op.index
        ))),
        (Some(output), None) => Ok(Resolved::Address {
            output,
            // Legacy address outputs do not store a creation score. Relative
            // locks against them are not meaningful; absolute locks still are.
            origin: TltOutputOrigin {
                blue_score: 0,
                median_time_secs: 0,
            },
        }),
        (None, Some(record)) => Ok(Resolved::Script {
            record,
            from_store: true,
        }),
        (None, None) => Err(StateError::MissingUtxo(format!(
            "{}:{}",
            op.tx_id.to_hex(),
            op.index
        ))),
    }
}

fn sighash(tx: &TltCovenantTx, auth: Option<&TxAuthContext>) -> Vec<u8> {
    match auth {
        Some(ctx) => tx.sighash_preimage_bound(&ctx.chain_id, &ctx.genesis),
        None => tx.sighash_preimage(),
    }
}

fn value_of(resolved: &Resolved) -> u64 {
    match resolved {
        Resolved::Address { output, .. } => output.value.as_base_units(),
        Resolved::Script { record, .. } => record.value.as_base_units(),
    }
}

fn script_of(resolved: &Resolved) -> Vec<u8> {
    match resolved {
        Resolved::Address { output, .. } => script_p2pkh(&output.address),
        Resolved::Script { record, .. } => record.script_pubkey.clone(),
    }
}

fn origin_of(resolved: &Resolved) -> TltOutputOrigin {
    match resolved {
        Resolved::Address { origin, .. } => *origin,
        Resolved::Script { record, .. } => record.origin(),
    }
}

fn hard(error: StateError) -> SelectError {
    SelectError { hard: true, error }
}

fn soft(error: StateError) -> SelectError {
    SelectError { hard: false, error }
}

/// Apply the covenant indexes chosen by [`selectable_covenant_fees`].
#[allow(clippy::too_many_arguments)]
pub fn apply_covenant_indexes(
    store: &StateStore,
    block: &Block,
    indexes: &[usize],
    auth: Option<&TxAuthContext>,
    blue_score: u64,
    median_time_secs: u64,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
    spent_in_block: &mut HashSet<OutPoint>,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
    coinbase_created: &HashSet<OutPoint>,
) -> Result<(), StateError> {
    let mut scripts: HashMap<OutPoint, TltCovenantUtxoRecord> = HashMap::new();
    for index in indexes {
        let tx = block
            .tlt_covenants
            .get(*index)
            .ok_or_else(|| StateError::InvalidTx("covenant index missing".into()))?;
        let inspection = inspect_covenant(
            store,
            tx,
            auth,
            blue_score,
            median_time_secs,
            spent_in_block,
            created_in_block,
            &scripts,
            coinbase_created,
        )
        .map_err(|err| err.error)?;
        let tx_id = tx.tx_id();
        for (op, resolved) in inspection.spends {
            match resolved {
                Resolved::Address { output, .. } => {
                    spend_utxo(
                        batch,
                        &op,
                        &output,
                        journal,
                        spent_in_block,
                        created_in_block,
                    );
                }
                Resolved::Script {
                    record, from_store, ..
                } => {
                    let key = covenant_utxo_key(&op);
                    batch.delete_cf(ColumnFamily::Meta, &key);
                    if from_store {
                        journal.tlt_covenant_spent.push((op, record));
                    } else {
                        journal
                            .tlt_covenant_created
                            .retain(|created| created != &op);
                        scripts.remove(&op);
                    }
                    spent_in_block.insert(op);
                }
            }
        }
        for (output_index, output) in tx.outputs.iter().enumerate() {
            let op = OutPoint {
                tx_id,
                index: output_index as u32,
            };
            if let Some(address) = p2pkh_address(&output.script_pubkey) {
                put_address_output(
                    store,
                    batch,
                    journal,
                    created_in_block,
                    op,
                    TxOut {
                        value: output.value,
                        address,
                    },
                )?;
            } else {
                let record = TltCovenantUtxoRecord {
                    value: output.value,
                    script_pubkey: output.script_pubkey.clone(),
                    origin_blue_score: blue_score,
                    origin_median_time_secs: median_time_secs,
                };
                put_script_output(store, batch, journal, &mut scripts, op, record)?;
            }
        }
    }
    Ok(())
}

fn put_address_output(
    store: &StateStore,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
    op: OutPoint,
    output: TxOut,
) -> Result<(), StateError> {
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
    let bytes = borsh::to_vec(&output).map_err(|err| StateError::Storage(err.to_string()))?;
    batch.put_cf(ColumnFamily::Utxo, &key, &bytes);
    journal.created.push(op);
    created_in_block.insert(op, output);
    Ok(())
}

fn put_script_output(
    store: &StateStore,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
    scripts: &mut HashMap<OutPoint, TltCovenantUtxoRecord>,
    op: OutPoint,
    record: TltCovenantUtxoRecord,
) -> Result<(), StateError> {
    if scripts.contains_key(&op) || journal.tlt_covenant_created.contains(&op) {
        return Err(StateError::DuplicateOutpoint(format!(
            "{}:{}",
            op.tx_id.to_hex(),
            op.index
        )));
    }
    let key = covenant_utxo_key(&op);
    if store.get_cf(ColumnFamily::Meta, &key)?.is_some() {
        return Err(StateError::DuplicateOutpoint(format!(
            "{}:{} (already a covenant utxo)",
            op.tx_id.to_hex(),
            op.index
        )));
    }
    let bytes = borsh::to_vec(&record).map_err(|err| StateError::Storage(err.to_string()))?;
    batch.put_cf(ColumnFamily::Meta, &key, &bytes);
    journal.tlt_covenant_created.push(op);
    scripts.insert(op, record);
    Ok(())
}

pub fn revert_covenant_outputs(
    batch: &mut WriteBatch,
    journal: &UtxoJournal,
) -> Result<(), StateError> {
    for op in journal.tlt_covenant_created.iter().rev() {
        batch.delete_cf(ColumnFamily::Meta, &covenant_utxo_key(op));
    }
    for (op, record) in journal.tlt_covenant_spent.iter().rev() {
        let bytes = borsh::to_vec(record).map_err(|err| StateError::Storage(err.to_string()))?;
        batch.put_cf(ColumnFamily::Meta, &covenant_utxo_key(op), &bytes);
    }
    Ok(())
}

/// Mempool check against the confirmed UTXO set. Returns the implicit fee.
pub fn validate_mempool_covenant(
    store: &StateStore,
    tx: &TltCovenantTx,
    reserved: &HashSet<OutPoint>,
    auth: Option<&TxAuthContext>,
    blue_score: u64,
    median_time_secs: u64,
) -> Result<u64, StateError> {
    let reserved_inputs = reserved.clone();
    // Own inputs are checked inside inspect; pre-seed conflicts from the pool.
    let inspection = inspect_covenant(
        store,
        tx,
        auth,
        blue_score,
        median_time_secs,
        &reserved_inputs,
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
    )
    .map_err(|err| err.error)?;
    // `reserved` already includes foreign spends. inspect treats them as conflicts.
    let _ = reserved_inputs;
    Ok(inspection.fee)
}

/// Predicted address outputs created by the coinbase and the transfers that will apply.
pub fn predicted_address_outputs(
    block: &Block,
    transfers: &[(&agora_types::Transaction, u64)],
    skip_existing_coinbase: bool,
) -> (
    HashMap<OutPoint, TxOut>,
    HashSet<OutPoint>,
    HashSet<OutPoint>,
) {
    let mut created = HashMap::new();
    let mut coinbase = HashSet::new();
    let mut spent = HashSet::new();
    if !skip_existing_coinbase {
        for tx in &block.transactions {
            if tx.inputs.is_empty() {
                insert_tx_outputs(tx, &mut created, Some(&mut coinbase));
            }
        }
    }
    for (tx, _) in transfers {
        insert_tx_outputs(tx, &mut created, None);
        for input in &tx.inputs {
            spent.insert(input.previous_outpoint);
        }
    }
    (created, coinbase, spent)
}

fn insert_tx_outputs(
    tx: &agora_types::Transaction,
    created: &mut HashMap<OutPoint, TxOut>,
    mut coinbase: Option<&mut HashSet<OutPoint>>,
) {
    let tx_id = tx.tx_id();
    for (index, output) in tx.outputs.iter().enumerate() {
        let op = OutPoint {
            tx_id,
            index: index as u32,
        };
        created.insert(op, output.clone());
        if let Some(set) = coinbase.as_deref_mut() {
            set.insert(op);
        }
    }
}
