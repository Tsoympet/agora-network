//! Native OVL execution boundary for Trident L1.
//!
//! This phase activates signed, gas-metered EOA calls against the canonical OVL
//! account ledger. Contract creation and non-empty call data are rejected until
//! the deterministic VM/state-storage transition lands; no legacy funded caller
//! or unsigned compact transaction is accepted.

use agora_crypto::verify_ovl_execution_bound;
use agora_types::{Amount, Hash, NativeAssetId, OvlExecutionTx};

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

pub const OVL_INTRINSIC_GAS: u64 = 21_000;
pub const OVL_EXECUTION_VERSION: u32 = 1;

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
    let mut from = load_account(store, NativeAssetId::OVL, &tx.from)?;
    let mut to = load_account(store, NativeAssetId::OVL, &tx.to)?;
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

    journal
        .before
        .push((NativeAssetId::OVL, tx.from, from.clone()));
    journal
        .before
        .push((NativeAssetId::OVL, tx.to, to.clone()));
    from.balance -= debit;
    from.nonce = from
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("OVL nonce overflow".into()))?;
    to.balance = recipient_balance;
    put_account_into(batch, NativeAssetId::OVL, &tx.from, &from)?;
    put_account_into(batch, NativeAssetId::OVL, &tx.to, &to)?;

    Ok(OvlExecutionReceipt {
        tx_id: tx.tx_id(),
        gas_used: OVL_INTRINSIC_GAS,
        fee_paid: fee,
        success: true,
    })
}

#[cfg(test)]
mod tests {
    use agora_crypto::{
        derive_bip44, seed_from_mnemonic, sign_ovl_execution_bound, Bip44Path,
    };
    use agora_types::{Address, Amount, Hash, OvlExecutionTx};

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
        let receipt =
            apply_ovl_execution(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
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
            },
            &mut batch,
            &mut journal,
        )
        .unwrap_err();
        assert!(err.to_string().contains("contract calls are not active"));
    }
}
