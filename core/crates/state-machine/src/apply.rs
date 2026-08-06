//! Apply / revert consensus-ordered blocks against multi-lane Trident state.

use std::collections::{HashMap, HashSet};

use agora_crypto::{address_from_pubkey, signer_address, verify_transaction_bound, PublicKeyBytes};
use agora_types::{
    Address, Amount, Block, Hash, NativeAssetId, OutPoint, Transaction, TransactionAcceptance,
    TxOut,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::acceptance::BlockAcceptanceRecord;
use crate::accounts::{
    apply_account_transfer_checked, load_account, put_account_into, AccountJournal, AccountState,
};
use crate::columns::ColumnFamily;
use crate::staking::{
    apply_signed_stake_tx, credit_fee_share_to_reward_pool, reward_pool_meta_key,
    snapshot_meta_keys, stake_meta_keys_touched, StakingParams,
};
use crate::store::WriteBatch;
use crate::utxo::outpoint_key;
use crate::{StateError, StateStore};

/// Result of applying one block's UTXO transition (journal + typed acceptance + batch).
pub struct BlockApplyResult {
    pub journal: UtxoJournal,
    pub acceptance: BlockAcceptanceRecord,
    pub batch: WriteBatch,
}

/// Network domain for transaction signatures (`chain_id` + genesis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxAuthContext {
    pub chain_id: String,
    pub genesis: Hash,
}

/// Journal of block mutations so a failed admission / reorg can roll back.
///
/// Accounting fields (`fees`, `subsidy`, `coinbase_total`) are persisted so unapply
/// never reverse-engineers fees from spent/created lists (which omit same-block
/// parent→child package edges). Account/stake lane snapshots restore OVL/DRC Meta.
#[derive(Debug, Default, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UtxoJournal {
    /// Outputs removed while applying (for revert: re-insert).
    pub spent: Vec<(OutPoint, TxOut)>,
    /// Outpoints created while applying (for revert: delete).
    pub created: Vec<OutPoint>,
    /// Exact transfer fee total (`Σ in − out`) for this block (TLT UTXO lane).
    pub fees: u64,
    /// Exact coinbase subsidy (`coinbase_total − fees`) credited to issued supply.
    pub subsidy: u64,
    /// Sum of coinbase outputs.
    pub coinbase_total: u64,
    /// OVL/DRC account states before Accepted account/stake lane ops.
    pub account_before: Vec<(NativeAssetId, Address, AccountState)>,
    /// Meta key snapshots before Accepted stake ops (`None` = key absent).
    pub stake_meta_before: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

/// Pre-v2 journal (spent + created only) for load migration.
#[derive(Debug, Clone, BorshDeserialize)]
struct LegacyUtxoJournal {
    spent: Vec<(OutPoint, TxOut)>,
    created: Vec<OutPoint>,
}

/// Pre-lane journal (UTXO accounting only).
#[derive(Debug, Clone, BorshDeserialize)]
struct UtxoJournalV2 {
    spent: Vec<(OutPoint, TxOut)>,
    created: Vec<OutPoint>,
    fees: u64,
    subsidy: u64,
    coinbase_total: u64,
}

impl UtxoJournal {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StateError> {
        if let Ok(j) = Self::try_from_slice(bytes) {
            return Ok(j);
        }
        if let Ok(v2) = UtxoJournalV2::try_from_slice(bytes) {
            return Ok(Self {
                spent: v2.spent,
                created: v2.created,
                fees: v2.fees,
                subsidy: v2.subsidy,
                coinbase_total: v2.coinbase_total,
                account_before: Vec::new(),
                stake_meta_before: Vec::new(),
            });
        }
        let legacy = LegacyUtxoJournal::try_from_slice(bytes)
            .map_err(|e| StateError::Storage(e.to_string()))?;
        Ok(Self {
            spent: legacy.spent,
            created: legacy.created,
            fees: 0,
            subsidy: 0,
            coinbase_total: 0,
            account_before: Vec::new(),
            stake_meta_before: Vec::new(),
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
    let result = apply_block_batched_with_auth(store, block, emission_reward, auth)?;
    store.write_batch(result.batch)?;
    Ok(result.journal)
}

/// Compute a block's UTXO transition without mutating the store.
///
/// Returns journal, typed acceptance outcomes, and an uncommitted [`WriteBatch`].
/// Callers can extend the batch (journal record, acceptance, issued-supply) and
/// commit atomically via [`StateStore::write_batch`].
pub fn apply_block_batched(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
) -> Result<BlockApplyResult, StateError> {
    apply_block_batched_with_auth(store, block, emission_reward, None)
}

/// How to treat duplicate / conflicting transactions when applying a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    /// Any double-spend or duplicate outpoint fails the block (mempool / lone validation).
    Strict,
    /// Skip txs whose inputs are already spent (or outputs already exist).
    ///
    /// Used when applying blues in virtual order so ordinary concurrent mempool
    /// duplicates and conflicting sibling spends do not invalidate a merge block.
    /// Consensus order determines the winner: first applied blue wins the UTXO.
    Virtual,
}

/// Like [`apply_block_batched`] with optional network-bound signature verification.
pub fn apply_block_batched_with_auth(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
    auth: Option<&TxAuthContext>,
) -> Result<BlockApplyResult, StateError> {
    apply_block_batched_mode(store, block, emission_reward, auth, ApplyMode::Strict)
}

/// Virtual-order apply: skip already-spent / duplicate-outpoint txs instead of failing.
///
/// Emits a [`BlockAcceptanceRecord`]: only [`TransactionAcceptance::Accepted`] txs
/// mutate UTXO and credit fees. Soft-skipped conflicts become `ConflictLost` or
/// `ExactDuplicate` after full structural/auth validation.
pub fn apply_block_batched_virtual(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
    auth: Option<&TxAuthContext>,
) -> Result<BlockApplyResult, StateError> {
    apply_block_batched_mode(store, block, emission_reward, auth, ApplyMode::Virtual)
}

fn apply_block_batched_mode(
    store: &StateStore,
    block: &Block,
    emission_reward: u64,
    auth: Option<&TxAuthContext>,
    mode: ApplyMode,
) -> Result<BlockApplyResult, StateError> {
    let mut batch = WriteBatch::new();
    let mut journal = UtxoJournal::default();
    let mut spent_in_block: HashSet<OutPoint> = HashSet::new();
    let mut created_in_block: HashMap<OutPoint, TxOut> = HashMap::new();
    let mut coinbase_created: HashSet<OutPoint> = HashSet::new();
    let mut coinbases = 0u32;
    let mut coinbase_total = 0u64;
    let mut applied_fees = 0u64;
    let mut statuses: Vec<TransactionAcceptance> =
        Vec::with_capacity(block.transactions.len());
    let mut accepted_tx_ids: HashSet<Hash> = HashSet::new();

    // Pre-scan transfers that will apply so coinbase budget matches Virtual skips.
    let transferable = selectable_transfers(store, block, auth, mode)?;
    for (_, fee) in &transferable {
        applied_fees = applied_fees
            .checked_add(*fee)
            .ok_or_else(|| StateError::InvalidTx("fee overflow".into()))?;
    }
    let coinbase_budget = emission_reward
        .checked_add(applied_fees)
        .ok_or_else(|| StateError::Coinbase("reward overflow".into()))?;

    let mut transfer_idx = 0usize;
    for tx in &block.transactions {
        if tx.inputs.is_empty() {
            coinbases += 1;
            if coinbases > 1 {
                return Err(StateError::Coinbase("multiple coinbase txs".into()));
            }
            if mode == ApplyMode::Virtual && coinbase_outpoints_exist(store, tx)? {
                // Identical sibling coinbase already created — skip (subsidy 0).
                statuses.push(TransactionAcceptance::ExactDuplicate);
                continue;
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
            accepted_tx_ids.insert(tx.tx_id());
            statuses.push(TransactionAcceptance::Accepted);
        } else if transfer_idx < transferable.len()
            && transferable[transfer_idx].0.tx_id() == tx.tx_id()
        {
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
            accepted_tx_ids.insert(tx.tx_id());
            statuses.push(TransactionAcceptance::Accepted);
            transfer_idx += 1;
        } else if mode == ApplyMode::Virtual {
            // Skipped duplicate / conflicting transfer — still fully validate auth
            // so garbage cannot hide in blue blocks unnoticed. Auth failure fails
            // the block (not a soft status): Invalid must not be silently dropped.
            let _ = verify_and_signer(tx, auth)?;
            if accepted_tx_ids.contains(&tx.tx_id()) {
                statuses.push(TransactionAcceptance::ExactDuplicate);
            } else {
                statuses.push(TransactionAcceptance::ConflictLost);
            }
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
            accepted_tx_ids.insert(tx.tx_id());
            statuses.push(TransactionAcceptance::Accepted);
        }
    }
    if coinbases == 0 {
        return Err(StateError::Coinbase("missing coinbase".into()));
    }

    journal.fees = applied_fees;
    journal.coinbase_total = coinbase_total;
    journal.subsidy = coinbase_total.saturating_sub(applied_fees.min(coinbase_total));

    // Fees credit only Accepted transfers (selectable set).
    debug_assert_eq!(
        statuses
            .iter()
            .filter(|s| s.credits_fees() && !matches!(s, TransactionAcceptance::Accepted))
            .count(),
        0
    );

    let (account_statuses, stake_statuses) =
        apply_trident_lanes(store, block, auth, mode, &mut batch, &mut journal)?;

    Ok(BlockApplyResult {
        journal,
        acceptance: BlockAcceptanceRecord {
            block_hash: Hash::ZERO, // filled by caller with block id
            statuses,
            account_statuses,
            stake_statuses,
        },
        batch,
    })
}

/// Soft-skippable lane conflicts under [`ApplyMode::Virtual`] (auth failures are hard).
fn is_lane_soft_conflict(err: &StateError) -> bool {
    match err {
        StateError::InvalidTx(msg) => {
            msg.contains("bad nonce")
                || msg.contains("bad stake nonce")
                || msg.contains("insufficient account balance")
                || msg.contains("insufficient liquid stake funds")
                || msg.contains("unknown validator")
                || msg.contains("already bonded")
                || msg.contains("self-transfer forbidden")
        }
        _ => false,
    }
}

fn params_for_stake_asset(asset: NativeAssetId) -> Result<StakingParams, StateError> {
    match asset {
        NativeAssetId::OVL => Ok(StakingParams::ovl_default()),
        NativeAssetId::DRC => Ok(StakingParams::drc_default()),
        NativeAssetId::TLT => Err(StateError::InvalidTx("TLT cannot be staked".into())),
    }
}

/// Apply OVL/DRC account transfers + stake ops; credit Accepted fees to reward pools.
fn apply_trident_lanes(
    store: &StateStore,
    block: &Block,
    auth: Option<&TxAuthContext>,
    mode: ApplyMode,
    batch: &mut WriteBatch,
    journal: &mut UtxoJournal,
) -> Result<(Vec<TransactionAcceptance>, Vec<TransactionAcceptance>), StateError> {
    if block.account_transfers.is_empty() && block.stake_ops.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    if !block.stake_ops.is_empty() && auth.is_none() {
        return Err(StateError::InvalidTx(
            "stake ops require network-bound auth".into(),
        ));
    }

    // Sequential visibility without committing the consensus batch early.
    let lane = store.cow_overlay();
    let mut account_statuses = Vec::with_capacity(block.account_transfers.len());
    let mut stake_statuses = Vec::with_capacity(block.stake_ops.len());
    let mut seen_account_ids: HashSet<Hash> = HashSet::new();
    let mut seen_stake_ids: HashSet<Hash> = HashSet::new();

    for tx in &block.account_transfers {
        let id = tx.transfer_id();
        let mut op_batch = WriteBatch::new();
        let mut acct_journal = AccountJournal::default();
        match apply_account_transfer_checked(&lane, tx, auth, &mut op_batch, &mut acct_journal) {
            Ok(()) => {
                if tx.fee.as_base_units() > 0 {
                    let pool_snap =
                        snapshot_meta_keys(&lane, &[reward_pool_meta_key(tx.asset)])?;
                    journal.stake_meta_before.extend(pool_snap);
                    credit_fee_share_to_reward_pool(
                        &lane,
                        &mut op_batch,
                        tx.asset,
                        tx.fee.as_base_units(),
                    )?;
                }
                lane.write_batch(op_batch.clone())?;
                batch.append(op_batch);
                journal.account_before.extend(acct_journal.before);
                seen_account_ids.insert(id);
                account_statuses.push(TransactionAcceptance::Accepted);
            }
            Err(err) if mode == ApplyMode::Virtual && is_lane_soft_conflict(&err) => {
                if seen_account_ids.contains(&id) {
                    account_statuses.push(TransactionAcceptance::ExactDuplicate);
                } else {
                    account_statuses.push(TransactionAcceptance::ConflictLost);
                }
            }
            Err(err) => return Err(err),
        }
    }

    for tx in &block.stake_ops {
        let id = tx.stake_tx_id();
        let ctx = auth.expect("stake auth checked above");
        let params = params_for_stake_asset(tx.asset)?;
        let actor_before = load_account(&lane, tx.asset, &tx.actor)?;
        let snap = snapshot_meta_keys(&lane, &stake_meta_keys_touched(tx))?;
        let mut op_batch = WriteBatch::new();
        match apply_signed_stake_tx(&lane, &mut op_batch, tx, ctx, &params) {
            Ok(()) => {
                lane.write_batch(op_batch.clone())?;
                batch.append(op_batch);
                journal.account_before.push((tx.asset, tx.actor, actor_before));
                journal.stake_meta_before.extend(snap);
                seen_stake_ids.insert(id);
                stake_statuses.push(TransactionAcceptance::Accepted);
            }
            Err(err) if mode == ApplyMode::Virtual && is_lane_soft_conflict(&err) => {
                if seen_stake_ids.contains(&id) {
                    stake_statuses.push(TransactionAcceptance::ExactDuplicate);
                } else {
                    stake_statuses.push(TransactionAcceptance::ConflictLost);
                }
            }
            Err(err) => return Err(err),
        }
    }

    Ok((account_statuses, stake_statuses))
}

/// Transfers that still have spendable inputs in `store` (plus in-block creates).
fn selectable_transfers<'a>(
    store: &StateStore,
    block: &'a Block,
    auth: Option<&TxAuthContext>,
    mode: ApplyMode,
) -> Result<Vec<(&'a Transaction, u64)>, StateError> {
    let mut out = Vec::new();
    let mut reserved: HashSet<OutPoint> = HashSet::new();
    let mut created: HashMap<OutPoint, TxOut> = HashMap::new();
    // Seed created with coinbase outs that will apply (non-duplicate).
    for tx in &block.transactions {
        if tx.inputs.is_empty() {
            if mode == ApplyMode::Virtual && coinbase_outpoints_exist(store, tx)? {
                continue;
            }
            let tx_id = tx.tx_id();
            for (index, o) in tx.outputs.iter().enumerate() {
                created.insert(
                    OutPoint {
                        tx_id,
                        index: index as u32,
                    },
                    o.clone(),
                );
            }
        }
    }
    for tx in &block.transactions {
        if tx.inputs.is_empty() {
            continue;
        }
        // Auth must be valid even for txs we may skip under Virtual
        // (except we only require auth when selecting — skipped later also verify).
        if mode == ApplyMode::Strict {
            let _ = verify_and_signer(tx, auth)?;
        }
        let mut ok = true;
        let mut input_value = 0u64;
        // Do not mutate `reserved` until the tx is fully selectable — a partial
        // soft-skip must not poison later spends of the same inputs.
        let mut pending_reserve: Vec<OutPoint> = Vec::new();
        for input in &tx.inputs {
            let op = input.previous_outpoint;
            if reserved.contains(&op) || pending_reserve.contains(&op) {
                ok = false;
                break;
            }
            let utxo = if let Some(c) = created.get(&op) {
                c.clone()
            } else {
                match store.get_cf(ColumnFamily::Utxo, &outpoint_key(&op))? {
                    Some(bytes) => TxOut::try_from_slice(&bytes)
                        .map_err(|e| StateError::Storage(e.to_string()))?,
                    None => {
                        ok = false;
                        break;
                    }
                }
            };
            input_value = input_value
                .checked_add(utxo.value.as_base_units())
                .ok_or_else(|| StateError::InvalidTx("input value overflow".into()))?;
            pending_reserve.push(op);
        }
        if !ok {
            if mode == ApplyMode::Strict {
                // Fall through to apply_transfer which reports the precise error.
                out.push((tx, 0));
            }
            continue;
        }
        let mut output_value = 0u64;
        for o in &tx.outputs {
            output_value = output_value
                .checked_add(o.value.as_base_units())
                .ok_or_else(|| StateError::InvalidTx("output value overflow".into()))?;
        }
        if input_value < output_value {
            // Semantic failure — not a soft-skippable duplicate/conflict.
            return Err(StateError::InvalidTx(format!(
                "insufficient funds: in={input_value} out={output_value}"
            )));
        }
        let fee = input_value - output_value;
        for op in pending_reserve {
            reserved.insert(op);
        }
        let tx_id = tx.tx_id();
        for (index, o) in tx.outputs.iter().enumerate() {
            created.insert(
                OutPoint {
                    tx_id,
                    index: index as u32,
                },
                o.clone(),
            );
        }
        out.push((tx, fee));
    }
    Ok(out)
}

fn coinbase_outpoints_exist(store: &StateStore, tx: &Transaction) -> Result<bool, StateError> {
    let tx_id = tx.tx_id();
    for index in 0..tx.outputs.len() {
        let op = OutPoint {
            tx_id,
            index: index as u32,
        };
        if store
            .get_cf(ColumnFamily::Utxo, &outpoint_key(&op))?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
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
    // Restore account/stake Meta in reverse so last-writer-wins for overlapping keys.
    for (asset, address, state) in journal.account_before.iter().rev() {
        put_account_into(&mut batch, *asset, address, state)?;
    }
    for (key, prior) in journal.stake_meta_before.iter().rev() {
        match prior {
            Some(value) => batch.put_cf(ColumnFamily::Meta, key, value),
            None => batch.delete_cf(ColumnFamily::Meta, key),
        }
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
        TransactionAcceptance,
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
            account_transfers: vec![],
            stake_ops: vec![],
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
            account_transfers: vec![],
            stake_ops: vec![],
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
            account_transfers: vec![],
            stake_ops: vec![],
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
            account_transfers: vec![],
            stake_ops: vec![],
        };
        assert!(matches!(
            apply_block(&store, &block, 50),
            Err(StateError::Coinbase(_))
        ));
    }

    #[test]
    fn virtual_skips_spent_input_without_poisoning_sibling() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis = GenesisBuilder::default()
            .with_premine_address(alice.address())
            .ignite(&store)
            .unwrap();
        let mut premine_txid = Hash::ZERO;
        store
            .for_each_cf(ColumnFamily::Utxo, |key, _| {
                if key.len() == 36 {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(&key[..32]);
                    premine_txid = Hash(h);
                }
                Ok(())
            })
            .unwrap();
        let premine_op = OutPoint {
            tx_id: premine_txid,
            index: 0,
        };
        let premine = load_utxo(&store, &premine_op).unwrap();

        // First transfer spends premine → bob.
        let mut win = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: premine_op,
            }],
            vec![TxOut {
                value: premine.value,
                address: bob.address(),
            }],
            1,
        );
        sign_transaction(&mut win, &alice).unwrap();
        let r1 = apply_block_batched_virtual(
            &store,
            &Block {
                header: BlockHeader {
                    version: 1,
                    parents: vec![genesis],
                    timestamp_ms: 1,
                    bits: 0,
                    nonce: 0,
                    tx_root: Hash::ZERO,
                },
                transactions: vec![
                    Transaction::unsigned(
                        1,
                        vec![],
                        vec![TxOut {
                            value: Amount::from_base_units(1),
                            address: Address::ZERO,
                        }],
                        1,
                    ),
                    win.clone(),
                ],
                account_transfers: vec![],
                stake_ops: vec![],
            },
            1,
            None,
        )
        .unwrap();
        store.write_batch(r1.batch).unwrap();
        assert_eq!(r1.journal.subsidy, 1);
        assert!(r1.acceptance.statuses.iter().all(|s| s.is_accepted()));

        // Block with: (1) conflicting spend of already-spent premine (multi-input
        // with a still-live bob out as second input — must not reserve bob's out),
        // (2) valid spend of bob's new out.
        let bob_op = OutPoint {
            tx_id: win.tx_id(),
            index: 0,
        };
        let mut conflict = Transaction::unsigned(
            1,
            vec![
                TxIn {
                    previous_outpoint: premine_op, // spent
                },
                TxIn {
                    previous_outpoint: bob_op, // live — must not be poisoned by soft-skip
                },
            ],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: alice.address(),
            }],
            2,
        );
        sign_transaction(&mut conflict, &alice).unwrap();

        let mut ok_spend = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: bob_op,
            }],
            vec![TxOut {
                value: premine.value,
                address: alice.address(),
            }],
            3,
        );
        sign_transaction(&mut ok_spend, &bob).unwrap();

        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis],
                timestamp_ms: 2,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![
                Transaction::unsigned(
                    1,
                    vec![],
                    vec![TxOut {
                        value: Amount::from_base_units(1),
                        address: Address::ZERO,
                    }],
                    2,
                ),
                conflict,
                ok_spend,
            ],
            account_transfers: vec![],
            stake_ops: vec![],
        };
        let result = apply_block_batched_virtual(&store, &block, 1, None).unwrap();
        store.write_batch(result.batch).unwrap();
        assert!(
            result.journal.created.iter().any(|op| op.index == 0),
            "valid bob→alice spend must apply after soft-skip"
        );
        assert_eq!(
            result.acceptance.statuses[1],
            TransactionAcceptance::ConflictLost
        );
        assert_eq!(
            result.acceptance.statuses[2],
            TransactionAcceptance::Accepted
        );
        assert_eq!(
            balance_of(&store, &alice.address())
                .unwrap()
                .as_base_units(),
            premine.value.as_base_units()
        );
    }

    #[test]
    fn virtual_rejects_insufficient_funds() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let genesis = GenesisBuilder::default()
            .with_premine_address(alice.address())
            .ignite(&store)
            .unwrap();
        let mut premine_txid = Hash::ZERO;
        store
            .for_each_cf(ColumnFamily::Utxo, |key, _| {
                if key.len() == 36 {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(&key[..32]);
                    premine_txid = Hash(h);
                }
                Ok(())
            })
            .unwrap();
        let premine = load_utxo(
            &store,
            &OutPoint {
                tx_id: premine_txid,
                index: 0,
            },
        )
        .unwrap();
        let mut bad = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: premine_txid,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(premine.value.as_base_units().saturating_add(1)),
                address: alice.address(),
            }],
            9,
        );
        sign_transaction(&mut bad, &alice).unwrap();
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![
                Transaction::unsigned(
                    1,
                    vec![],
                    vec![TxOut {
                        value: Amount::from_base_units(1),
                        address: Address::ZERO,
                    }],
                    1,
                ),
                bad,
            ],
            account_transfers: vec![],
            stake_ops: vec![],
        };
        assert!(matches!(
            apply_block_batched_virtual(&store, &block, 1, None),
            Err(StateError::InvalidTx(_))
        ));
    }

    #[test]
    fn account_transfer_fee_credits_reward_pool_on_accept() {
        use agora_crypto::sign_account_transfer_bound;
        use agora_types::{AccountTransfer, NativeAssetId};
        use crate::accounts::{credit_account_into, load_account};
        use crate::staking::load_reward_pool;

        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let genesis_hash = GenesisBuilder::default()
            .with_premine_address(alice.address())
            .ignite(&store)
            .unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-trident-testnet-1".into(),
            genesis: genesis_hash,
        };

        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let mut transfer = AccountTransfer::unsigned_with_fee(
            NativeAssetId::OVL,
            alice.address(),
            bob.address(),
            Amount::from_base_units(100),
            Amount::from_base_units(7),
            0,
        );
        sign_account_transfer_bound(&mut transfer, &alice, &auth.chain_id, &auth.genesis).unwrap();

        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::ZERO,
                address: Address::ZERO,
            }],
            1,
        );
        let txs = vec![coinbase];
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_hash],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: txs,
            account_transfers: vec![transfer],
            stake_ops: vec![],
        };
        let mut block = block;
        block.header.tx_root = block.compute_body_root();

        let result = apply_block_batched_with_auth(&store, &block, 0, Some(&auth)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            result.acceptance.account_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &alice.address())
                .unwrap()
                .balance,
            893
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &bob.address())
                .unwrap()
                .balance,
            100
        );
        assert_eq!(load_reward_pool(&store, NativeAssetId::OVL).unwrap(), 7);
    }

    #[test]
    fn stake_op_in_block_body_bonds_validator() {
        use agora_crypto::sign_stake_tx_bound;
        use agora_types::{NativeAssetId, SignedStakeTx};
        use crate::accounts::{credit_account_into, load_account};
        use crate::staking::{load_validator, StakingParams};

        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let op = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let genesis_hash = GenesisBuilder::default()
            .with_premine_address(op.address())
            .ignite(&store)
            .unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-trident-testnet-1".into(),
            genesis: genesis_hash,
        };
        let params = StakingParams::ovl_default();
        let bond = params.min_self_bond;

        let mut batch = WriteBatch::new();
        credit_account_into(
            &mut batch,
            &store,
            NativeAssetId::OVL,
            &op.address(),
            Amount::from_base_units(bond * 2),
        )
        .unwrap();
        store.write_batch(batch).unwrap();

        let mut stake = SignedStakeTx::unsigned_bond(
            NativeAssetId::OVL,
            op.address(),
            bond,
            op.public_key_bytes().to_vec(),
            op.address(),
            0,
            0,
        );
        sign_stake_tx_bound(&mut stake, &op, &auth.chain_id, &auth.genesis).unwrap();

        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::ZERO,
                address: Address::ZERO,
            }],
            2,
        );
        let mut block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_hash],
                timestamp_ms: 2,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![coinbase],
            account_transfers: vec![],
            stake_ops: vec![stake],
        };
        block.header.tx_root = block.compute_body_root();

        let result = apply_block_batched_with_auth(&store, &block, 0, Some(&auth)).unwrap();
        store.write_batch(result.batch).unwrap();
        assert_eq!(
            result.acceptance.stake_statuses,
            vec![TransactionAcceptance::Accepted]
        );
        let val = load_validator(&store, NativeAssetId::OVL, &op.address())
            .unwrap()
            .expect("bonded");
        assert_eq!(val.self_bond, bond);
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &op.address())
                .unwrap()
                .balance,
            bond
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &op.address())
                .unwrap()
                .nonce,
            1
        );
    }

    #[test]
    fn multi_lane_body_root_differs_from_tx_merkle() {
        use agora_types::{AccountTransfer, NativeAssetId};
        let coinbase = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::ZERO,
                address: Address::ZERO,
            }],
            0,
        );
        let txs = vec![coinbase];
        let utxo_only = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(&txs),
            },
            txs.clone(),
        );
        assert_eq!(utxo_only.compute_body_root(), Block::compute_tx_root(&txs));

        let mut multi = utxo_only.clone();
        multi.account_transfers.push(AccountTransfer::unsigned(
            NativeAssetId::OVL,
            Address::ZERO,
            Address([1u8; 20]),
            Amount::from_base_units(1),
            0,
        ));
        assert_ne!(multi.compute_body_root(), Block::compute_tx_root(&txs));
    }
}
