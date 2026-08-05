//! Transaction acceptance layer.
//!
//! This module is the single authority for deciding which transactions in blue
//! blocks become part of the UTXO set. Block color alone never implies acceptance.
//!
//! Pipeline (per blue block, in GHOSTDAG blue order):
//! 1. Fully validate every transaction independent of conflict outcome.
//! 2. Resolve exact duplicates and input conflicts by blue order (first accepted wins).
//! 3. Produce a deterministic accepted/rejected bitmap.
//! 4. Sum fees only from accepted non-coinbase transactions.
//! 5. Accept the coinbase only when its outputs equal `subsidy + accepted_fees`.

use std::collections::{HashMap, HashSet};

use agora_crypto::{signer_address, verify_transaction};
use agora_types::{
    AcceptanceBitmap, Amount, Block, Hash, NetworkFingerprint, OutPoint, Transaction, TxOut,
};

use crate::emission::EmissionSchedule;
use crate::ghostdag::OrderedBlock;
use crate::ConsensusError;

/// Read-only UTXO lookup used during validation (implemented by the state machine).
pub trait UtxoView {
    fn get(&self, outpoint: &OutPoint) -> Option<TxOut>;
}

/// In-memory UTXO map for tests and ephemeral overlays.
#[derive(Debug, Default, Clone)]
pub struct MemoryUtxoView {
    utxos: HashMap<OutPoint, TxOut>,
}

impl MemoryUtxoView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, outpoint: OutPoint, output: TxOut) {
        self.utxos.insert(outpoint, output);
    }

    pub fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut> {
        self.utxos.remove(outpoint)
    }

    pub fn len(&self) -> usize {
        self.utxos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.utxos.is_empty()
    }
}

impl UtxoView for MemoryUtxoView {
    fn get(&self, outpoint: &OutPoint) -> Option<TxOut> {
        self.utxos.get(outpoint).cloned()
    }
}

impl UtxoView for HashMap<OutPoint, TxOut> {
    fn get(&self, outpoint: &OutPoint) -> Option<TxOut> {
        HashMap::get(self, outpoint).cloned()
    }
}

/// Single UTXO mutation produced by accepted transactions (applied atomically by state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtxoJournalOp {
    Create { outpoint: OutPoint, output: TxOut },
    Spend { outpoint: OutPoint },
}

/// Why a transaction failed validation or conflict resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxRejectReason {
    /// Failed structural / auth / value checks (independent of conflicts).
    Invalid(&'static str),
    /// Exact same `tx_id` was already accepted earlier in blue order.
    DuplicateExact,
    /// Shares an input outpoint with an earlier accepted transaction.
    InputConflict,
    /// Coinbase output total does not match subsidy + accepted fees.
    InvalidCoinbaseReward,
}

/// Per-transaction validation + acceptance outcome.
///
/// `structurally_valid` is set even when the tx is later rejected for conflicts,
/// satisfying “fully validate independent of conflict outcome.”
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxAcceptanceOutcome {
    pub tx_id: Hash,
    pub index: u32,
    pub is_coinbase: bool,
    pub structurally_valid: bool,
    pub accepted: bool,
    pub fee: Amount,
    pub reject_reason: Option<TxRejectReason>,
}

/// Acceptance result for one blue block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAcceptance {
    pub block_hash: Hash,
    pub blue_score: u64,
    pub bitmap: AcceptanceBitmap,
    pub outcomes: Vec<TxAcceptanceOutcome>,
    /// Fees from accepted non-coinbase transactions only.
    pub accepted_fees: Amount,
    /// Block subsidy supplied by the caller (emission or genesis premine).
    pub subsidy: Amount,
    /// `subsidy + accepted_fees` — the only legal coinbase claim.
    pub coinbase_reward: Amount,
}

/// Full acceptance run over a blue-ordered sequence of blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceResult {
    pub blocks: Vec<BlockAcceptance>,
    pub journal: Vec<UtxoJournalOp>,
}

/// One blue block ready for acceptance, with the subsidy that coinbase may claim
/// before fees (emission schedule or genesis premine).
#[derive(Debug, Clone)]
pub struct BlueBlockInput {
    pub ordered: OrderedBlock,
    pub block: Block,
    pub subsidy: Amount,
}

/// Mutable overlay tracking spends/creates during a multi-block acceptance run.
///
/// Spent outputs keep their values so later conflicting transactions can still be
/// fully validated (fee / value checks) independent of conflict outcome.
struct UtxoOverlay<'a, V: UtxoView> {
    base: &'a V,
    created: HashMap<OutPoint, TxOut>,
    spent: HashSet<OutPoint>,
    spent_values: HashMap<OutPoint, TxOut>,
}

impl<'a, V: UtxoView> UtxoOverlay<'a, V> {
    fn new(base: &'a V) -> Self {
        Self {
            base,
            created: HashMap::new(),
            spent: HashSet::new(),
            spent_values: HashMap::new(),
        }
    }

    /// Lookup for validation — includes outputs already spent by accepted txs.
    fn get_for_validation(&self, outpoint: &OutPoint) -> Option<TxOut> {
        if let Some(out) = self.created.get(outpoint) {
            return Some(out.clone());
        }
        if let Some(out) = self.spent_values.get(outpoint) {
            return Some(out.clone());
        }
        self.base.get(outpoint)
    }

    fn is_spent(&self, outpoint: &OutPoint) -> bool {
        self.spent.contains(outpoint)
    }

    fn apply_accepted(&mut self, tx: &Transaction) -> Vec<UtxoJournalOp> {
        let tx_id = tx.tx_id();
        let mut ops = Vec::new();
        for input in &tx.inputs {
            let op = input.previous_outpoint;
            if let Some(value) = self.get_for_validation(&op) {
                self.spent_values.insert(op, value);
            }
            self.spent.insert(op);
            self.created.remove(&op);
            ops.push(UtxoJournalOp::Spend { outpoint: op });
        }
        for (index, output) in tx.outputs.iter().enumerate() {
            let outpoint = OutPoint {
                tx_id,
                index: index as u32,
            };
            self.created.insert(outpoint, output.clone());
            self.spent.remove(&outpoint);
            self.spent_values.remove(&outpoint);
            ops.push(UtxoJournalOp::Create {
                outpoint,
                output: output.clone(),
            });
        }
        ops
    }
}

/// Run the acceptance layer over blue-ordered blocks.
///
/// Red blocks must not be supplied. Within each block, transactions are considered
/// in index order; across blocks, GHOSTDAG blue order is authoritative.
pub fn accept_blue_blocks<V: UtxoView>(
    inputs: &[BlueBlockInput],
    utxo: &V,
    fingerprint: &NetworkFingerprint,
) -> Result<AcceptanceResult, ConsensusError> {
    let mut overlay = UtxoOverlay::new(utxo);
    let mut accepted_tx_ids: HashSet<Hash> = HashSet::new();
    let mut blocks_out = Vec::with_capacity(inputs.len());

    for input in inputs {
        if !input.ordered.is_blue {
            return Err(ConsensusError::Acceptance(
                "acceptance layer only processes blue blocks".into(),
            ));
        }
        if input.ordered.hash != input.block.id() {
            return Err(ConsensusError::Acceptance(
                "ordered block hash does not match block payload".into(),
            ));
        }
        if !input.block.verify_tx_root() {
            return Err(ConsensusError::Acceptance(
                "block tx_root does not commit to transactions".into(),
            ));
        }
        let expected_subsidy = expected_subsidy_for(fingerprint, input);
        if input.subsidy != expected_subsidy {
            return Err(ConsensusError::Acceptance(format!(
                "subsidy {} does not match network schedule {}",
                input.subsidy.as_base_units(),
                expected_subsidy.as_base_units()
            )));
        }

        let block_acceptance =
            accept_single_blue_block(input, &mut overlay, &mut accepted_tx_ids, fingerprint)?;
        blocks_out.push(block_acceptance);
    }

    // Rebuild journal deterministically from accepted outcomes.
    let mut rebuild = UtxoOverlay::new(utxo);
    let mut journal = Vec::new();
    for (input, acceptance) in inputs.iter().zip(blocks_out.iter()) {
        for (idx, outcome) in acceptance.outcomes.iter().enumerate() {
            if outcome.accepted {
                journal.extend(rebuild.apply_accepted(&input.block.transactions[idx]));
            }
        }
    }

    Ok(AcceptanceResult {
        blocks: blocks_out,
        journal,
    })
}

fn accept_single_blue_block<'a, V: UtxoView>(
    input: &BlueBlockInput,
    overlay: &mut UtxoOverlay<'a, V>,
    accepted_tx_ids: &mut HashSet<Hash>,
    fingerprint: &NetworkFingerprint,
) -> Result<BlockAcceptance, ConsensusError> {
    let txs = &input.block.transactions;
    if txs.is_empty() {
        return Err(ConsensusError::Acceptance(
            "blue block must contain at least a coinbase transaction".into(),
        ));
    }

    // Coinbase slot is exclusive: index 0 must have no inputs. Never treat it as a
    // regular spend (that previously allowed apply-then-reject without rollback).
    if !txs[0].inputs.is_empty() {
        return Err(ConsensusError::Acceptance(
            "block transaction 0 must be coinbase (no inputs)".into(),
        ));
    }

    let mut outcomes = Vec::with_capacity(txs.len());
    let mut accepted_fees = Amount::ZERO;

    // Placeholder for coinbase; finalized after fees are known.
    outcomes.push(TxAcceptanceOutcome {
        tx_id: txs[0].tx_id(),
        index: 0,
        is_coinbase: true,
        structurally_valid: false,
        accepted: false,
        fee: Amount::ZERO,
        reject_reason: None,
    });

    // Pass 1: fully validate every non-coinbase tx, then resolve conflicts by blue order.
    for (index, tx) in txs.iter().enumerate().skip(1) {
        let (structurally_valid, fee, validation_reject) =
            validate_regular_tx(tx, overlay, fingerprint);

        let mut reject_reason = validation_reject;
        // Start from pure validation; conflict resolution may still reject.
        let mut accepted = structurally_valid && reject_reason.is_none();

        if accepted {
            let tx_id = tx.tx_id();
            if accepted_tx_ids.contains(&tx_id) {
                accepted = false;
                reject_reason = Some(TxRejectReason::DuplicateExact);
            } else if tx
                .inputs
                .iter()
                .any(|i| overlay.is_spent(&i.previous_outpoint))
            {
                accepted = false;
                reject_reason = Some(TxRejectReason::InputConflict);
            }
        }

        if accepted {
            accepted_tx_ids.insert(tx.tx_id());
            overlay.apply_accepted(tx);
            accepted_fees = accepted_fees
                .checked_add(fee)
                .ok_or_else(|| ConsensusError::Acceptance("fee overflow".into()))?;
        }

        outcomes.push(TxAcceptanceOutcome {
            tx_id: tx.tx_id(),
            index: index as u32,
            is_coinbase: false,
            structurally_valid,
            accepted,
            fee: if structurally_valid {
                fee
            } else {
                Amount::ZERO
            },
            reject_reason,
        });
    }

    let coinbase_reward = input
        .subsidy
        .checked_add(accepted_fees)
        .ok_or_else(|| ConsensusError::Acceptance("coinbase reward overflow".into()))?;

    // Pass 2: coinbase must claim exactly subsidy + accepted fees.
    let coinbase = &txs[0];
    let (cb_valid, cb_reject, cb_accepted) = validate_coinbase(coinbase, coinbase_reward);

    if cb_accepted && accepted_tx_ids.contains(&coinbase.tx_id()) {
        outcomes[0] = TxAcceptanceOutcome {
            tx_id: coinbase.tx_id(),
            index: 0,
            is_coinbase: true,
            structurally_valid: cb_valid,
            accepted: false,
            fee: Amount::ZERO,
            reject_reason: Some(TxRejectReason::DuplicateExact),
        };
    } else if cb_accepted {
        accepted_tx_ids.insert(coinbase.tx_id());
        overlay.apply_accepted(coinbase);
        outcomes[0] = TxAcceptanceOutcome {
            tx_id: coinbase.tx_id(),
            index: 0,
            is_coinbase: true,
            structurally_valid: cb_valid,
            accepted: true,
            fee: Amount::ZERO,
            reject_reason: None,
        };
    } else {
        outcomes[0] = TxAcceptanceOutcome {
            tx_id: coinbase.tx_id(),
            index: 0,
            is_coinbase: true,
            structurally_valid: cb_valid,
            accepted: false,
            fee: Amount::ZERO,
            reject_reason: cb_reject,
        };
    }

    // Bitmap is derived from final outcomes only.
    let flags: Vec<bool> = outcomes.iter().map(|o| o.accepted).collect();

    Ok(BlockAcceptance {
        block_hash: input.ordered.hash,
        blue_score: input.ordered.blue_score,
        bitmap: AcceptanceBitmap::from_bools(&flags),
        outcomes,
        accepted_fees,
        subsidy: input.subsidy,
        coinbase_reward,
    })
}

/// Subsidy implied by the network fingerprint (premine for genesis, emission otherwise).
fn expected_subsidy_for(fingerprint: &NetworkFingerprint, input: &BlueBlockInput) -> Amount {
    if input.block.header.parents.is_empty() {
        return Amount::from_base_units(fingerprint.premine);
    }
    let schedule = EmissionSchedule {
        initial_reward: fingerprint.initial_reward,
        halving_interval: fingerprint.halving_interval,
    };
    Amount::from_base_units(schedule.reward_at_blue_score(input.ordered.blue_score))
}

/// Fully validate a regular (non-coinbase) transaction.
///
/// Does not decide duplicate / input-conflict rejection — that is blue-order policy.
fn validate_regular_tx<V: UtxoView>(
    tx: &Transaction,
    overlay: &UtxoOverlay<'_, V>,
    fingerprint: &NetworkFingerprint,
) -> (bool, Amount, Option<TxRejectReason>) {
    if tx.inputs.is_empty() {
        return (
            false,
            Amount::ZERO,
            Some(TxRejectReason::Invalid("regular tx requires inputs")),
        );
    }
    if tx.outputs.is_empty() {
        return (
            false,
            Amount::ZERO,
            Some(TxRejectReason::Invalid("tx requires outputs")),
        );
    }
    if verify_transaction(tx, fingerprint).is_err() {
        return (
            false,
            Amount::ZERO,
            Some(TxRejectReason::Invalid("invalid signature")),
        );
    }
    let Ok(signer) = signer_address(tx, fingerprint) else {
        return (
            false,
            Amount::ZERO,
            Some(TxRejectReason::Invalid("invalid signature")),
        );
    };

    let mut seen = HashSet::new();
    for input in &tx.inputs {
        if !seen.insert(input.previous_outpoint) {
            return (
                false,
                Amount::ZERO,
                Some(TxRejectReason::Invalid("duplicate input within tx")),
            );
        }
    }

    let mut input_total = Amount::ZERO;
    for input in &tx.inputs {
        let Some(utxo) = overlay.get_for_validation(&input.previous_outpoint) else {
            return (
                false,
                Amount::ZERO,
                Some(TxRejectReason::Invalid("missing utxo")),
            );
        };
        // Critical: signature alone is not enough — signer must own each spent UTXO.
        if utxo.address != signer {
            return (
                false,
                Amount::ZERO,
                Some(TxRejectReason::Invalid("signer does not own utxo")),
            );
        }
        input_total = match input_total.checked_add(utxo.value) {
            Some(v) => v,
            None => {
                return (
                    false,
                    Amount::ZERO,
                    Some(TxRejectReason::Invalid("input value overflow")),
                );
            }
        };
    }

    let mut output_total = Amount::ZERO;
    for output in &tx.outputs {
        if output.value.as_base_units() == 0 {
            return (
                false,
                Amount::ZERO,
                Some(TxRejectReason::Invalid("zero-value output")),
            );
        }
        output_total = match output_total.checked_add(output.value) {
            Some(v) => v,
            None => {
                return (
                    false,
                    Amount::ZERO,
                    Some(TxRejectReason::Invalid("output value overflow")),
                );
            }
        };
    }

    let Some(fee) = input_total.checked_sub(output_total) else {
        return (
            false,
            Amount::ZERO,
            Some(TxRejectReason::Invalid("outputs exceed inputs")),
        );
    };

    (true, fee, None)
}

fn validate_coinbase(
    tx: &Transaction,
    expected_reward: Amount,
) -> (bool, Option<TxRejectReason>, bool) {
    if !tx.inputs.is_empty() {
        return (
            false,
            Some(TxRejectReason::Invalid("coinbase must have no inputs")),
            false,
        );
    }
    if tx.outputs.is_empty() {
        return (
            false,
            Some(TxRejectReason::Invalid("coinbase requires outputs")),
            false,
        );
    }
    if !tx.public_key.is_empty() || !tx.signature.is_empty() {
        return (
            false,
            Some(TxRejectReason::Invalid(
                "coinbase must not carry signature material",
            )),
            false,
        );
    }

    let mut total = Amount::ZERO;
    for output in &tx.outputs {
        if output.value.as_base_units() == 0 {
            return (
                false,
                Some(TxRejectReason::Invalid("zero-value coinbase output")),
                false,
            );
        }
        match total.checked_add(output.value) {
            Some(v) => total = v,
            None => {
                return (
                    false,
                    Some(TxRejectReason::Invalid("coinbase value overflow")),
                    false,
                );
            }
        }
    }

    // Structurally valid even when reward mismatches — mismatch is a distinct reject reason.
    if total != expected_reward {
        return (true, Some(TxRejectReason::InvalidCoinbaseReward), false);
    }

    (true, None, true)
}

/// Fees from accepted non-coinbase transactions in a [`BlockAcceptance`].
pub fn fees_from_accepted(block: &BlockAcceptance) -> Amount {
    block.accepted_fees
}

/// Coinbase reward compatible with accepted fees: `subsidy + accepted_fees`.
pub fn coinbase_reward(subsidy: Amount, accepted_fees: Amount) -> Option<Amount> {
    subsidy.checked_add(accepted_fees)
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Address, BlockHeader, TxIn};

    use super::*;
    use crate::emission::EmissionSchedule;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn fingerprint(genesis: Hash, premine: Amount) -> NetworkFingerprint {
        let emission = EmissionSchedule::default();
        NetworkFingerprint {
            network_name: "agora-test".into(),
            network_id: 1,
            genesis_hash: genesis,
            ghostdag_k: 18,
            max_supply: Amount::from_whole(100_000_000).unwrap().as_base_units(),
            premine: premine.as_base_units(),
            initial_reward: emission.initial_reward,
            halving_interval: emission.halving_interval,
        }
    }

    fn ordered(hash: Hash, blue_score: u64) -> OrderedBlock {
        OrderedBlock {
            hash,
            blue_score,
            is_blue: true,
        }
    }

    fn make_block(parents: Vec<Hash>, txs: Vec<Transaction>) -> Block {
        let tx_root = Block::compute_tx_root(&txs);
        Block {
            header: BlockHeader {
                version: 1,
                parents,
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root,
            },
            transactions: txs,
        }
    }

    fn coinbase(value: Amount, address: Address, nonce: u64) -> Transaction {
        Transaction::unsigned(1, vec![], vec![TxOut { value, address }], nonce)
    }

    fn signed_spend(
        fp: &NetworkFingerprint,
        utxo_outpoint: OutPoint,
        utxo_value: Amount,
        to: Address,
        send: Amount,
        change: Address,
        nonce: u64,
    ) -> Transaction {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let change_value = utxo_value.checked_sub(send).unwrap();
        // Leave 1 base unit as fee when possible.
        let change_amt = if change_value.as_base_units() > 1 {
            Amount::from_base_units(change_value.as_base_units() - 1)
        } else {
            change_value
        };
        let mut outputs = vec![TxOut {
            value: send,
            address: to,
        }];
        if change_amt.as_base_units() > 0 {
            outputs.push(TxOut {
                value: change_amt,
                address: change,
            });
        }
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: utxo_outpoint,
            }],
            outputs,
            nonce,
        );
        sign_transaction(&mut tx, &kp, fp).unwrap();
        tx
    }

    #[test]
    fn accepts_valid_tx_and_fees_only_from_accepted() {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let alice = kp.address();
        let bob = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();

        let premine = Amount::from_whole(10).unwrap();
        let genesis_cb = coinbase(premine, alice, 0);
        let genesis = make_block(vec![], vec![genesis_cb.clone()]);
        let genesis_hash = genesis.id();
        let fp = fingerprint(genesis_hash, premine);

        let mut utxos = MemoryUtxoView::new();
        let genesis_txid = genesis_cb.tx_id();
        utxos.insert(
            OutPoint {
                tx_id: genesis_txid,
                index: 0,
            },
            genesis_cb.outputs[0].clone(),
        );

        let spend = signed_spend(
            &fp,
            OutPoint {
                tx_id: genesis_txid,
                index: 0,
            },
            premine,
            bob,
            Amount::from_whole(3).unwrap(),
            alice,
            1,
        );
        let fee = Amount::from_base_units(1);
        let subsidy = Amount::from_base_units(EmissionSchedule::default().initial_reward);
        let cb = coinbase(subsidy.checked_add(fee).unwrap(), alice, 2);
        let block = make_block(vec![genesis_hash], vec![cb, spend]);
        let block_hash = block.id();

        let result = accept_blue_blocks(
            &[
                BlueBlockInput {
                    ordered: ordered(genesis_hash, 1),
                    block: genesis,
                    subsidy: premine,
                },
                BlueBlockInput {
                    ordered: ordered(block_hash, 2),
                    block,
                    subsidy,
                },
            ],
            &utxos,
            &fp,
        )
        .unwrap();

        assert_eq!(result.blocks.len(), 2);
        let b1 = &result.blocks[1];
        assert!(b1.bitmap.is_accepted(0));
        assert!(b1.bitmap.is_accepted(1));
        assert_eq!(b1.accepted_fees, fee);
        assert_eq!(b1.coinbase_reward, subsidy.checked_add(fee).unwrap());
        assert_eq!(fees_from_accepted(b1), fee);
    }

    #[test]
    fn conflict_resolved_by_blue_order_first_accepted_wins() {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let alice = kp.address();
        let bob = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();
        let carol = derive_bip44(&seed, &Bip44Path::external(2))
            .unwrap()
            .address();

        let premine = Amount::from_whole(10).unwrap();
        let genesis_cb = coinbase(premine, alice, 0);
        let genesis = make_block(vec![], vec![genesis_cb.clone()]);
        let genesis_hash = genesis.id();
        let fp = fingerprint(genesis_hash, premine);
        let genesis_txid = genesis_cb.tx_id();

        let mut utxos = MemoryUtxoView::new();
        utxos.insert(
            OutPoint {
                tx_id: genesis_txid,
                index: 0,
            },
            genesis_cb.outputs[0].clone(),
        );

        let outpoint = OutPoint {
            tx_id: genesis_txid,
            index: 0,
        };
        let tx_a = signed_spend(
            &fp,
            outpoint,
            premine,
            bob,
            Amount::from_whole(4).unwrap(),
            alice,
            10,
        );
        let tx_b = signed_spend(
            &fp,
            outpoint,
            premine,
            carol,
            Amount::from_whole(5).unwrap(),
            alice,
            11,
        );

        let fee_a = Amount::from_base_units(1);
        let subsidy = Amount::from_base_units(EmissionSchedule::default().initial_reward);

        let cb_a = coinbase(subsidy.checked_add(fee_a).unwrap(), alice, 20);
        let block_a = make_block(vec![genesis_hash], vec![cb_a, tx_a]);
        let hash_a = block_a.id();

        // tx_b will be rejected for conflict ⇒ coinbase may claim subsidy only.
        let cb_b = coinbase(subsidy, alice, 21);
        let block_b = make_block(vec![genesis_hash], vec![cb_b, tx_b]);
        let hash_b = block_b.id();

        let result = accept_blue_blocks(
            &[
                BlueBlockInput {
                    ordered: ordered(genesis_hash, 1),
                    block: genesis,
                    subsidy: premine,
                },
                BlueBlockInput {
                    ordered: ordered(hash_a, 2),
                    block: block_a,
                    subsidy,
                },
                BlueBlockInput {
                    ordered: ordered(hash_b, 3),
                    block: block_b,
                    subsidy,
                },
            ],
            &utxos,
            &fp,
        )
        .unwrap();

        let a = &result.blocks[1];
        let b = &result.blocks[2];
        assert!(a.bitmap.is_accepted(1), "first spender accepted");
        assert!(!b.bitmap.is_accepted(1), "conflicting spender rejected");
        assert_eq!(
            b.outcomes[1].reject_reason,
            Some(TxRejectReason::InputConflict)
        );
        assert!(b.outcomes[1].structurally_valid);
        assert_eq!(b.accepted_fees, Amount::ZERO);
        assert!(b.bitmap.is_accepted(0));
    }

    #[test]
    fn exact_duplicate_rejected_by_blue_order() {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let alice = kp.address();
        let bob = derive_bip44(&seed, &Bip44Path::external(1))
            .unwrap()
            .address();

        let premine = Amount::from_whole(10).unwrap();
        let genesis_cb = coinbase(premine, alice, 0);
        let genesis = make_block(vec![], vec![genesis_cb.clone()]);
        let genesis_hash = genesis.id();
        let fp = fingerprint(genesis_hash, premine);
        let genesis_txid = genesis_cb.tx_id();

        let mut utxos = MemoryUtxoView::new();
        utxos.insert(
            OutPoint {
                tx_id: genesis_txid,
                index: 0,
            },
            genesis_cb.outputs[0].clone(),
        );

        let tx = signed_spend(
            &fp,
            OutPoint {
                tx_id: genesis_txid,
                index: 0,
            },
            premine,
            bob,
            Amount::from_whole(2).unwrap(),
            alice,
            3,
        );
        let fee = Amount::from_base_units(1);
        let subsidy = Amount::from_base_units(EmissionSchedule::default().initial_reward);

        let cb1 = coinbase(subsidy.checked_add(fee).unwrap(), alice, 30);
        let block1 = make_block(vec![genesis_hash], vec![cb1, tx.clone()]);
        let hash1 = block1.id();

        let cb2 = coinbase(subsidy, alice, 31);
        let block2 = make_block(vec![hash1], vec![cb2, tx]);
        let hash2 = block2.id();

        let result = accept_blue_blocks(
            &[
                BlueBlockInput {
                    ordered: ordered(genesis_hash, 1),
                    block: genesis,
                    subsidy: premine,
                },
                BlueBlockInput {
                    ordered: ordered(hash1, 2),
                    block: block1,
                    subsidy,
                },
                BlueBlockInput {
                    ordered: ordered(hash2, 3),
                    block: block2,
                    subsidy,
                },
            ],
            &utxos,
            &fp,
        )
        .unwrap();

        assert!(result.blocks[1].bitmap.is_accepted(1));
        assert!(!result.blocks[2].bitmap.is_accepted(1));
        assert_eq!(
            result.blocks[2].outcomes[1].reject_reason,
            Some(TxRejectReason::DuplicateExact)
        );
        assert!(result.blocks[2].outcomes[1].structurally_valid);
    }

    #[test]
    fn rejects_spend_when_signer_does_not_own_utxo() {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let owner = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let thief = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let premine = Amount::from_whole(10).unwrap();
        let genesis_cb = coinbase(premine, owner.address(), 0);
        let genesis = make_block(vec![], vec![genesis_cb.clone()]);
        let genesis_hash = genesis.id();
        let fp = fingerprint(genesis_hash, premine);
        let mut utxos = MemoryUtxoView::new();
        utxos.insert(
            OutPoint {
                tx_id: genesis_cb.tx_id(),
                index: 0,
            },
            genesis_cb.outputs[0].clone(),
        );

        // Thief signs a spend of owner's UTXO — must fail ownership check.
        let mut stolen = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: genesis_cb.tx_id(),
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_whole(1).unwrap(),
                address: thief.address(),
            }],
            77,
        );
        sign_transaction(&mut stolen, &thief, &fp).unwrap();

        let subsidy = Amount::from_base_units(EmissionSchedule::default().initial_reward);
        let cb = coinbase(subsidy, owner.address(), 40);
        let block = make_block(vec![genesis_hash], vec![cb, stolen]);
        let hash = block.id();

        let result = accept_blue_blocks(
            &[
                BlueBlockInput {
                    ordered: ordered(genesis_hash, 1),
                    block: genesis,
                    subsidy: premine,
                },
                BlueBlockInput {
                    ordered: ordered(hash, 2),
                    block,
                    subsidy,
                },
            ],
            &utxos,
            &fp,
        )
        .unwrap();

        let outcome = &result.blocks[1].outcomes[1];
        assert!(!outcome.structurally_valid);
        assert!(!outcome.accepted);
        assert_eq!(
            outcome.reject_reason,
            Some(TxRejectReason::Invalid("signer does not own utxo"))
        );
    }

    #[test]
    fn invalid_tx_fully_validated_but_rejected() {
        let alice = Address([1u8; 20]);
        let premine = Amount::from_whole(1).unwrap();
        let genesis_cb = coinbase(premine, alice, 0);
        let genesis = make_block(vec![], vec![genesis_cb.clone()]);
        let genesis_hash = genesis.id();
        let fp = fingerprint(genesis_hash, premine);

        let mut utxos = MemoryUtxoView::new();
        utxos.insert(
            OutPoint {
                tx_id: genesis_cb.tx_id(),
                index: 0,
            },
            genesis_cb.outputs[0].clone(),
        );

        let bad = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: genesis_cb.tx_id(),
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: alice,
            }],
            99,
        );
        let subsidy = Amount::from_base_units(EmissionSchedule::default().initial_reward);
        let cb = coinbase(subsidy, alice, 40);
        let block = make_block(vec![genesis_hash], vec![cb, bad]);
        let hash = block.id();

        let result = accept_blue_blocks(
            &[
                BlueBlockInput {
                    ordered: ordered(genesis_hash, 1),
                    block: genesis,
                    subsidy: premine,
                },
                BlueBlockInput {
                    ordered: ordered(hash, 2),
                    block,
                    subsidy,
                },
            ],
            &utxos,
            &fp,
        )
        .unwrap();

        let outcome = &result.blocks[1].outcomes[1];
        assert!(!outcome.structurally_valid);
        assert!(!outcome.accepted);
        assert!(!result.blocks[1].bitmap.is_accepted(1));
        assert_eq!(result.blocks[1].accepted_fees, Amount::ZERO);
    }
}
