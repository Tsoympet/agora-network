//! Apply / revert consensus-ordered blocks against the UTXO set.

use std::collections::{HashMap, HashSet};

use agora_crypto::{signer_address, verify_transaction};
use agora_types::{Amount, Block, OutPoint, Transaction, TxOut};
use borsh::BorshDeserialize;

use crate::columns::ColumnFamily;
use crate::utxo::outpoint_key;
use crate::{StateError, StateStore};

/// Journal of UTXO mutations so a failed admission can roll back.
#[derive(Debug, Default, Clone)]
pub struct UtxoJournal {
    /// Outputs removed while applying (for revert: re-insert).
    pub spent: Vec<(OutPoint, TxOut)>,
    /// Outpoints created while applying (for revert: delete).
    pub created: Vec<OutPoint>,
}

/// Apply all transactions in `block` to `cf_utxo`.
///
/// Rules:
/// - At most one coinbase (`inputs` empty); its outputs must total ≤ `coinbase_reward`
/// - Non-coinbase txs must verify secp256k1 auth; each spent UTXO must belong to the signer
/// - Input value ≥ output value (difference is implicit fee / burn)
/// - No double-spends within the block or against the live set
pub fn apply_block(
    store: &StateStore,
    block: &Block,
    coinbase_reward: u64,
) -> Result<UtxoJournal, StateError> {
    let mut journal = UtxoJournal::default();
    let mut spent_in_block: HashSet<OutPoint> = HashSet::new();
    let mut created_in_block: HashMap<OutPoint, TxOut> = HashMap::new();
    let mut coinbases = 0u32;

    for tx in &block.transactions {
        if tx.inputs.is_empty() {
            coinbases += 1;
            if coinbases > 1 {
                return Err(StateError::Coinbase("multiple coinbase txs".into()));
            }
            apply_coinbase(store, tx, coinbase_reward, &mut journal, &mut created_in_block)?;
        } else {
            apply_transfer(
                store,
                tx,
                &mut journal,
                &mut spent_in_block,
                &mut created_in_block,
            )?;
        }
    }

    Ok(journal)
}

fn apply_coinbase(
    store: &StateStore,
    tx: &Transaction,
    coinbase_reward: u64,
    journal: &mut UtxoJournal,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
) -> Result<(), StateError> {
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
    create_outputs(store, tx, journal, created_in_block)
}

fn apply_transfer(
    store: &StateStore,
    tx: &Transaction,
    journal: &mut UtxoJournal,
    spent_in_block: &mut HashSet<OutPoint>,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
) -> Result<(), StateError> {
    verify_transaction(tx).map_err(|e| StateError::InvalidTx(e.to_string()))?;
    let signer = signer_address(tx).map_err(|e| StateError::InvalidTx(e.to_string()))?;

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
        spend_utxo(store, &op, &out, journal, spent_in_block, created_in_block)?;
    }
    create_outputs(store, tx, journal, created_in_block)
}

fn load_utxo(store: &StateStore, op: &OutPoint) -> Result<TxOut, StateError> {
    let key = outpoint_key(op);
    let bytes = store
        .get_cf(ColumnFamily::Utxo, &key)?
        .ok_or_else(|| {
            StateError::MissingUtxo(format!("{}:{}", op.tx_id.to_hex(), op.index))
        })?;
    TxOut::try_from_slice(&bytes).map_err(|e| StateError::Storage(e.to_string()))
}

fn spend_utxo(
    store: &StateStore,
    op: &OutPoint,
    out: &TxOut,
    journal: &mut UtxoJournal,
    spent_in_block: &mut HashSet<OutPoint>,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
) -> Result<(), StateError> {
    let key = outpoint_key(op);
    store.delete_cf(ColumnFamily::Utxo, &key)?;
    if created_in_block.remove(op).is_some() {
        // Created earlier in this same block — drop from journal so revert is a no-op.
        journal.created.retain(|c| c != op);
    } else {
        journal.spent.push((*op, out.clone()));
    }
    spent_in_block.insert(*op);
    Ok(())
}

fn create_outputs(
    store: &StateStore,
    tx: &Transaction,
    journal: &mut UtxoJournal,
    created_in_block: &mut HashMap<OutPoint, TxOut>,
) -> Result<(), StateError> {
    let tx_id = tx.tx_id();
    for (index, out) in tx.outputs.iter().enumerate() {
        let op = OutPoint {
            tx_id,
            index: index as u32,
        };
        let key = outpoint_key(&op);
        let bytes = borsh::to_vec(out).map_err(|e| StateError::Storage(e.to_string()))?;
        store.put_cf(ColumnFamily::Utxo, &key, &bytes)?;
        journal.created.push(op);
        created_in_block.insert(op, out.clone());
    }
    Ok(())
}

/// Reverse a successful [`apply_block`] journal (best-effort atomicity for scaffolds).
pub fn revert_journal(store: &StateStore, journal: &UtxoJournal) -> Result<(), StateError> {
    for op in journal.created.iter().rev() {
        store.delete_cf(ColumnFamily::Utxo, &outpoint_key(op))?;
    }
    for (op, out) in journal.spent.iter().rev() {
        let bytes = borsh::to_vec(out).map_err(|e| StateError::Storage(e.to_string()))?;
        store.put_cf(ColumnFamily::Utxo, &outpoint_key(op), &bytes)?;
    }
    Ok(())
}

/// Sum of all UTXO values for `address` (same scan used by RPC balances).
pub fn balance_of(store: &StateStore, address: &agora_types::Address) -> Result<Amount, StateError> {
    let mut total = Amount::ZERO;
    store.for_each_cf(ColumnFamily::Utxo, |_key, value| {
        let out = TxOut::try_from_slice(value)
            .map_err(|e| StateError::Storage(e.to_string()))?;
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
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
    use agora_types::{Address, Amount, Block, BlockHeader, Hash, OutPoint, Transaction, TxIn, TxOut};

    use super::*;
    use crate::genesis::GenesisBuilder;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn apply_transfer_spends_premine_and_reverts() {
        let store = StateStore::open("/tmp/agora-apply-utxo").unwrap();
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

        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_hash],
                timestamp_ms: 1,
                bits: 0,
                nonce: 0,
                tx_root: Block::compute_tx_root(std::slice::from_ref(&tx)),
            },
            transactions: vec![tx.clone()],
        };

        let journal = apply_block(&store, &block, 0).unwrap();
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
    fn rejects_oversized_coinbase() {
        let store = StateStore::open("/tmp/agora-apply-coinbase").unwrap();
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
