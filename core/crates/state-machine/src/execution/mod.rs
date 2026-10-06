//! Native OVL execution boundary for Trident L1.
//!
//! This phase activates signed, gas-metered EOA calls against the canonical OVL
//! account ledger. Contract creation and non-empty call data are rejected until
//! the deterministic VM/state-storage transition lands; no legacy funded caller
//! or unsigned compact transaction is accepted.

use agora_crypto::verify_ovl_execution_bound;
use agora_ovl_evm::{apply_raw_transaction, SelectedBlock};
use agora_types::{Hash, OvlExecutionTx, OVL_EXECUTION_RAW_EVM_VERSION};

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::ovl_evm_state::{load_ovl_evm_world, put_ovl_evm_world_into};
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

pub const OVL_INTRINSIC_GAS: u64 = 21_000;
pub const OVL_EXECUTION_VERSION: u32 = 1;

/// Selected-order fields copied into the EVM block environment.
///
/// The EVM block number is the count of applied blocks that carried a raw
/// transaction, not a consensus height and not a finality input.
#[derive(Clone, Debug)]
pub struct OvlSelectedOrder {
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub timestamp: u64,
}

/// Deterministic outcome produced by an accepted execution envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OvlExecutionReceipt {
    pub tx_id: Hash,
    pub gas_used: u64,
    pub fee_paid: u64,
    pub success: bool,
}

pub fn execution_fee(tx: &OvlExecutionTx) -> Result<u64, StateError> {
    OVL_INTRINSIC_GAS
        .checked_mul(tx.max_fee_per_gas)
        .ok_or_else(|| StateError::InvalidTx("OVL execution fee overflow".into()))
}

/// Apply one signed OVL execution envelope to canonical account state.
///
/// Only EOA value calls are active in this experimental boundary. Rejecting
/// non-empty data prevents a no-op scaffold from masquerading as contract
/// execution while preserving the signed/gas-metered wire format for VM work.
pub fn apply_ovl_execution(
    store: &StateStore,
    tx: &OvlExecutionTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<OvlExecutionReceipt, StateError> {
    apply_ovl_execution_with_block(store, tx, auth, None, batch, journal)
}

pub fn apply_ovl_execution_with_block(
    store: &StateStore,
    tx: &OvlExecutionTx,
    auth: &TxAuthContext,
    selected: Option<&OvlSelectedOrder>,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<OvlExecutionReceipt, StateError> {
    if tx.version == OVL_EXECUTION_RAW_EVM_VERSION {
        return apply_raw_ethereum_execution(store, tx, selected, batch);
    }
    let asset = tx.execution_asset();
    if !asset.is_programmable_execution_asset() {
        return Err(StateError::InvalidTx(
            "programmable execution is restricted to OVL".into(),
        ));
    }
    if tx.version != OVL_EXECUTION_VERSION {
        return Err(StateError::InvalidTx(format!(
            "unsupported OVL execution version {}",
            tx.version
        )));
    }
    if tx.to == agora_types::Address::ZERO {
        return Err(StateError::InvalidTx(
            "OVL contract creation is not active".into(),
        ));
    }
    if tx.from == tx.to {
        return Err(StateError::InvalidTx(
            "OVL self-execution is forbidden".into(),
        ));
    }
    if !tx.data.is_empty() {
        return Err(StateError::InvalidTx(
            "OVL contract calls are not active".into(),
        ));
    }
    if tx.gas_limit < OVL_INTRINSIC_GAS {
        return Err(StateError::InvalidTx(format!(
            "OVL gas limit {} below intrinsic {}",
            tx.gas_limit, OVL_INTRINSIC_GAS
        )));
    }
    if tx.max_fee_per_gas == 0 {
        return Err(StateError::InvalidTx(
            "OVL max_fee_per_gas must be positive".into(),
        ));
    }
    verify_ovl_execution_bound(tx, &auth.chain_id, &auth.genesis)
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;

    let fee = execution_fee(tx)?;
    let debit = tx
        .value
        .as_base_units()
        .checked_add(fee)
        .ok_or_else(|| StateError::InvalidTx("OVL value+fee overflow".into()))?;
    let mut from = load_account(store, asset, &tx.from)?;
    let mut to = load_account(store, asset, &tx.to)?;
    if from.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad OVL execution nonce: got {} expected {}",
            tx.nonce, from.nonce
        )));
    }
    if from.balance < debit {
        return Err(StateError::InvalidTx(
            "insufficient OVL execution balance".into(),
        ));
    }
    let recipient_balance = to
        .balance
        .checked_add(tx.value.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("OVL recipient overflow".into()))?;

    journal.before.push((asset, tx.from, from.clone()));
    journal.before.push((asset, tx.to, to.clone()));
    from.balance -= debit;
    from.nonce = from
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("OVL nonce overflow".into()))?;
    to.balance = recipient_balance;
    put_account_into(batch, asset, &tx.from, &from)?;
    put_account_into(batch, asset, &tx.to, &to)?;

    Ok(OvlExecutionReceipt {
        tx_id: tx.tx_id(),
        gas_used: OVL_INTRINSIC_GAS,
        fee_paid: fee,
        success: true,
    })
}

fn apply_raw_ethereum_execution(
    store: &StateStore,
    tx: &OvlExecutionTx,
    selected: Option<&OvlSelectedOrder>,
    batch: &mut WriteBatch,
) -> Result<OvlExecutionReceipt, StateError> {
    if tx.execution_asset() != agora_types::NativeAssetId::OVL {
        return Err(StateError::InvalidTx(
            "programmable execution is restricted to OVL".into(),
        ));
    }
    let selected = selected.ok_or_else(|| {
        StateError::InvalidTx("raw EVM execution requires a selected block".into())
    })?;
    let mut world = load_ovl_evm_world(store)?;
    world
        .require_active()
        .map_err(|err| StateError::InvalidTx(err.to_string()))?;
    if world.block.hash != selected.hash {
        world
            .open_block(SelectedBlock {
                number: world.block.number.saturating_add(1),
                hash: selected.hash,
                parent_hash: selected.parent_hash,
                timestamp: selected.timestamp,
                beneficiary: world.block.beneficiary,
            })
            .map_err(|err| StateError::InvalidTx(err.to_string()))?;
    }
    let receipt = apply_raw_transaction(&mut world, &tx.data)
        .map_err(|err| StateError::InvalidTx(err.to_string()))?;
    put_ovl_evm_world_into(batch, &world);
    Ok(OvlExecutionReceipt {
        tx_id: tx.tx_id(),
        gas_used: receipt.gas_used,
        // Base fee is burned inside the world. The tip is credited to the
        // configured beneficiary by the engine. Do not also credit the reward pool.
        fee_paid: 0,
        success: receipt.status,
    })
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_ovl_execution_bound, Bip44Path};
    use agora_types::{Address, Amount, Hash, NativeAssetId, OvlExecutionTx};

    use super::*;
    use crate::accounts::{credit_account_into, load_account};

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn signed_eoa_call_charges_intrinsic_gas_and_value() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([1; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::OVL,
            &alice.address(),
            Amount::from_base_units(100_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();

        let mut tx = OvlExecutionTx::unsigned(
            alice.address(),
            bob.address(),
            Amount::from_base_units(1_000),
            OVL_INTRINSIC_GAS,
            2,
            0,
            vec![],
        );
        sign_ovl_execution_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let receipt = apply_ovl_execution(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        assert_eq!(receipt.gas_used, OVL_INTRINSIC_GAS);
        assert_eq!(receipt.fee_paid, 42_000);
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &alice.address())
                .unwrap()
                .balance,
            57_000
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &bob.address())
                .unwrap()
                .balance,
            1_000
        );
    }

    #[test]
    fn contract_payloads_rejected_until_vm_activation() {
        let tx = OvlExecutionTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            Amount::ZERO,
            OVL_INTRINSIC_GAS,
            1,
            0,
            vec![0x60, 0x00],
        );
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let err = apply_ovl_execution(
            &store,
            &tx,
            &TxAuthContext {
                chain_id: "agora-dev".into(),
                genesis: Hash::ZERO,
                data_availability_network_fingerprint: None,
            },
            &mut batch,
            &mut journal,
        )
        .unwrap_err();
        assert!(err.to_string().contains("contract calls are not active"));
    }

    #[test]
    fn drc_balance_cannot_fund_the_ovl_execution_lane() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let bob = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([1; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &alice.address(),
            Amount::from_base_units(100_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();

        let mut tx = OvlExecutionTx::unsigned(
            alice.address(),
            bob.address(),
            Amount::from_base_units(1_000),
            OVL_INTRINSIC_GAS,
            2,
            0,
            vec![],
        );
        sign_ovl_execution_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let error = apply_ovl_execution(&store, &tx, &auth, &mut batch, &mut journal).unwrap_err();

        assert!(error
            .to_string()
            .contains("insufficient OVL execution balance"));
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &alice.address())
                .unwrap()
                .balance,
            100_000
        );
        assert_eq!(
            load_account(&store, NativeAssetId::OVL, &alice.address())
                .unwrap()
                .balance,
            0
        );
        assert!(journal.before.is_empty());
    }

    #[test]
    fn raw_ethereum_lane_is_dev_gated_and_isolated_from_drc_and_tlt() {
        use agora_ovl_evm::{
            dev_signing_key, ethereum_address_from_signing_key, sign_eip1559, OvlEvmWorld,
        };
        use agora_types::OvlWei;

        use crate::monetary::TridentMonetaryPolicy;
        use crate::ovl_evm_state::{load_ovl_evm_world, put_ovl_evm_world_into, OVL_EVM_WORLD_KEY};
        use crate::staking::snapshot_meta_keys;
        use crate::{
            compose_trident_state_root, ignite_trident_supply, revert_journal, UtxoJournal,
        };

        let store = StateStore::open_in_memory();
        let mut ignited = WriteBatch::new();
        ignite_trident_supply(&mut ignited, &TridentMonetaryPolicy::default()).unwrap();
        store.write_batch(ignited).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([1; 32]),
            data_availability_network_fingerprint: None,
        };
        let selected = OvlSelectedOrder {
            hash: [9u8; 32],
            parent_hash: [0u8; 32],
            timestamp: 1_700_000_000,
        };
        let inactive = OvlExecutionTx::raw_ethereum(vec![0x02, 0xc0]);
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let err = apply_ovl_execution_with_block(
            &store,
            &inactive,
            &auth,
            Some(&selected),
            &mut batch,
            &mut journal,
        )
        .unwrap_err();
        assert!(err.to_string().contains("inactive") || err.to_string().contains("dev gate"));

        let key = dev_signing_key();
        let caller = ethereum_address_from_signing_key(&key);
        let mut world = OvlEvmWorld::dev();
        world
            .fund(caller, OvlWei::from_u128(10u128.pow(18)))
            .unwrap();
        let mut activate = WriteBatch::new();
        put_ovl_evm_world_into(&mut activate, &world);
        store.write_batch(activate).unwrap();
        let root_before = compose_trident_state_root(&store, &Hash([1; 32])).unwrap();
        let drc_before = load_account(&store, NativeAssetId::DRC, &Address([4; 20]))
            .unwrap()
            .balance;
        let tlt_note = b"tlt-coinbase-is-not-an-evm-transaction";
        let foreign = OvlExecutionTx::raw_ethereum(tlt_note.to_vec());
        let mut batch = WriteBatch::new();
        let err = apply_ovl_execution_with_block(
            &store,
            &foreign,
            &auth,
            Some(&selected),
            &mut batch,
            &mut AccountJournal::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("rejected") || err.to_string().contains("transaction"));

        let raw = sign_eip1559(
            &key,
            world.chain_id,
            0,
            0,
            u128::from(world.block.base_fee),
            21_000,
            Some([0x44; 20]),
            {
                let mut value = [0u8; 32];
                value[31] = 1;
                value
            },
            &[],
            &[],
        );
        let tx = OvlExecutionTx::raw_ethereum(raw);
        let snap = snapshot_meta_keys(&store, &[OVL_EVM_WORLD_KEY.to_vec()]).unwrap();
        let mut batch = WriteBatch::new();
        let receipt = apply_ovl_execution_with_block(
            &store,
            &tx,
            &auth,
            Some(&selected),
            &mut batch,
            &mut AccountJournal::default(),
        )
        .unwrap();
        assert!(receipt.success);
        assert_eq!(receipt.fee_paid, 0);
        assert!(receipt.gas_used >= 21_000);
        store.write_batch(batch).unwrap();
        let committed = load_ovl_evm_world(&store).unwrap();
        assert!(committed.burned > OvlWei::ZERO);
        assert_eq!(committed.block.hash, selected.hash);
        assert_eq!(committed.block.timestamp, selected.timestamp);
        assert_ne!(
            compose_trident_state_root(&store, &Hash([1; 32])).unwrap(),
            root_before
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &Address([4; 20]))
                .unwrap()
                .balance,
            drc_before
        );

        let revert = UtxoJournal {
            stake_meta_before: snap,
            ..UtxoJournal::default()
        };
        revert_journal(&store, &revert).unwrap();
        assert_eq!(
            compose_trident_state_root(&store, &Hash([1; 32])).unwrap(),
            root_before
        );
        let restarted = load_ovl_evm_world(&store).unwrap();
        assert_eq!(restarted.execution_subroot(), world.execution_subroot());

        let other = StateStore::open_in_memory();
        let mut other_batch = WriteBatch::new();
        ignite_trident_supply(&mut other_batch, &TridentMonetaryPolicy::default()).unwrap();
        put_ovl_evm_world_into(&mut other_batch, &world);
        other.write_batch(other_batch).unwrap();
        let mut replay = WriteBatch::new();
        apply_ovl_execution_with_block(
            &other,
            &tx,
            &auth,
            Some(&selected),
            &mut replay,
            &mut AccountJournal::default(),
        )
        .unwrap();
        other.write_batch(replay).unwrap();
        assert_eq!(
            load_ovl_evm_world(&store).unwrap().execution_subroot(),
            world.execution_subroot()
        );
        // The reverted store matches the preimage. Replay the first store and
        // compare the two committed worlds.
        let mut again = WriteBatch::new();
        apply_ovl_execution_with_block(
            &store,
            &tx,
            &auth,
            Some(&selected),
            &mut again,
            &mut AccountJournal::default(),
        )
        .unwrap();
        store.write_batch(again).unwrap();
        assert_eq!(
            load_ovl_evm_world(&store).unwrap().execution_subroot(),
            load_ovl_evm_world(&other).unwrap().execution_subroot()
        );
    }
}
