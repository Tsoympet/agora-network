//! Native DRC payment transition and deterministic transport outbox.
//!
//! This is an L1 account payment module. It does not import the historical
//! district-chain ledger, PoW, bridge attestors, or transport cryptography.

use agora_crypto::verify_drc_payment_bound;
use agora_types::{DrcPaymentOutboxEvent, DrcPaymentTx, Hash, NativeAssetId};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const SEEN_PREFIX: &[u8] = b"payment/drc/seen/";
const INVOICE_PREFIX: &[u8] = b"payment/drc/invoice/";
const OUTBOX_PREFIX: &[u8] = b"payment/drc/outbox/";
const PAYMENT_ROOT_KEY: &[u8] = b"payment/drc/root";
pub const DRC_PAYMENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrcPaymentReceipt {
    pub payment_id: Hash,
    pub fee_paid: u64,
}

pub fn payment_seen_key(payment_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(SEEN_PREFIX.len() + 32);
    key.extend_from_slice(SEEN_PREFIX);
    key.extend_from_slice(payment_id.as_bytes());
    key
}

/// Invoice uniqueness is scoped to the recipient merchant.
pub fn payment_invoice_key(to: &agora_types::Address, invoice_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(INVOICE_PREFIX.len() + 20 + 32);
    key.extend_from_slice(INVOICE_PREFIX);
    key.extend_from_slice(&to.0);
    key.extend_from_slice(invoice_id.as_bytes());
    key
}

pub fn payment_outbox_key(payment_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(OUTBOX_PREFIX.len() + 32);
    key.extend_from_slice(OUTBOX_PREFIX);
    key.extend_from_slice(payment_id.as_bytes());
    key
}

/// Meta keys changed by an accepted payment, for reorg snapshots.
pub fn payment_meta_keys(tx: &DrcPaymentTx) -> Vec<Vec<u8>> {
    let id = tx.payment_id();
    let mut keys = vec![
        payment_seen_key(&id),
        payment_outbox_key(&id),
        PAYMENT_ROOT_KEY.to_vec(),
    ];
    if tx.invoice_id != Hash::ZERO {
        keys.push(payment_invoice_key(&tx.to, &tx.invoice_id));
    }
    keys
}

pub fn load_drc_outbox_event(
    store: &StateStore,
    payment_id: &Hash,
) -> Result<Option<DrcPaymentOutboxEvent>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &payment_outbox_key(payment_id))? else {
        return Ok(None);
    };
    DrcPaymentOutboxEvent::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| StateError::Storage(e.to_string()))
}

pub fn list_drc_outbox(
    store: &StateStore,
    limit: usize,
) -> Result<Vec<DrcPaymentOutboxEvent>, StateError> {
    let mut events = Vec::new();
    for (_, bytes) in store
        .scan_prefix(ColumnFamily::Meta, OUTBOX_PREFIX)?
        .into_iter()
        .take(limit)
    {
        events.push(
            DrcPaymentOutboxEvent::try_from_slice(&bytes)
                .map_err(|e| StateError::Storage(e.to_string()))?,
        );
    }
    Ok(events)
}

/// Bounded rolling commitment to accepted payment metadata and outbox events.
pub fn drc_payment_root(store: &StateStore) -> Result<Hash, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, PAYMENT_ROOT_KEY)? else {
        return Ok(Hash::hash_borsh(&(
            b"agora-drc-payment-root-v1",
            Hash::ZERO,
        )));
    };
    if bytes.len() != 32 {
        return Err(StateError::Storage(
            "invalid DRC payment root length".into(),
        ));
    }
    let mut root = [0u8; 32];
    root.copy_from_slice(&bytes);
    Ok(Hash(root))
}

/// Validate and apply one signed DRC payment.
///
/// Every duplicate/index/overflow check runs before writes are appended.
pub fn apply_drc_payment(
    store: &StateStore,
    tx: &DrcPaymentTx,
    auth: &TxAuthContext,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcPaymentReceipt, StateError> {
    if tx.version != DRC_PAYMENT_VERSION {
        return Err(StateError::InvalidTx(format!(
            "unsupported DRC payment version {}",
            tx.version
        )));
    }
    if tx.amount.as_base_units() == 0 {
        return Err(StateError::InvalidTx("zero DRC payment".into()));
    }
    if tx.from == tx.to {
        return Err(StateError::InvalidTx("DRC self-payment forbidden".into()));
    }
    verify_drc_payment_bound(tx, &auth.chain_id, &auth.genesis)
        .map_err(|e| StateError::InvalidTx(e.to_string()))?;

    let payment_id = tx.payment_id();
    if store
        .get_cf(ColumnFamily::Meta, &payment_seen_key(&payment_id))?
        .is_some()
    {
        return Err(StateError::InvalidTx("duplicate DRC payment id".into()));
    }
    if tx.invoice_id != Hash::ZERO
        && store
            .get_cf(
                ColumnFamily::Meta,
                &payment_invoice_key(&tx.to, &tx.invoice_id),
            )?
            .is_some()
    {
        return Err(StateError::InvalidTx(
            "duplicate DRC merchant invoice".into(),
        ));
    }

    let mut from = load_account(store, NativeAssetId::DRC, &tx.from)?;
    let mut to = load_account(store, NativeAssetId::DRC, &tx.to)?;
    if from.nonce != tx.nonce {
        return Err(StateError::InvalidTx(format!(
            "bad DRC payment nonce: got {} expected {}",
            tx.nonce, from.nonce
        )));
    }
    let debit = tx
        .amount
        .as_base_units()
        .checked_add(tx.fee.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("DRC payment amount+fee overflow".into()))?;
    if from.balance < debit {
        return Err(StateError::InvalidTx(
            "insufficient DRC payment balance".into(),
        ));
    }
    let recipient_balance = to
        .balance
        .checked_add(tx.amount.as_base_units())
        .ok_or_else(|| StateError::InvalidTx("DRC payment recipient overflow".into()))?;
    let event = DrcPaymentOutboxEvent::from_tx(tx);
    let event_bytes = borsh::to_vec(&event).map_err(|e| StateError::Storage(e.to_string()))?;
    let prior_payment_root = drc_payment_root(store)?;
    let next_payment_root =
        Hash::hash_borsh(&(b"agora-drc-payment-root-v1", prior_payment_root, &event));

    journal
        .before
        .push((NativeAssetId::DRC, tx.from, from.clone()));
    journal.before.push((NativeAssetId::DRC, tx.to, to.clone()));
    from.balance -= debit;
    from.nonce = from
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC payment nonce overflow".into()))?;
    to.balance = recipient_balance;
    put_account_into(batch, NativeAssetId::DRC, &tx.from, &from)?;
    put_account_into(batch, NativeAssetId::DRC, &tx.to, &to)?;
    batch.put_cf(ColumnFamily::Meta, &payment_seen_key(&payment_id), &[1]);
    if tx.invoice_id != Hash::ZERO {
        batch.put_cf(
            ColumnFamily::Meta,
            &payment_invoice_key(&tx.to, &tx.invoice_id),
            payment_id.as_bytes(),
        );
    }
    batch.put_cf(
        ColumnFamily::Meta,
        &payment_outbox_key(&payment_id),
        &event_bytes,
    );
    batch.put_cf(
        ColumnFamily::Meta,
        PAYMENT_ROOT_KEY,
        next_payment_root.as_bytes(),
    );

    Ok(DrcPaymentReceipt {
        payment_id,
        fee_paid: tx.fee.as_base_units(),
    })
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_drc_payment_bound, Bip44Path};
    use agora_types::{Amount, DrcPaymentTx};

    use super::*;
    use crate::accounts::{credit_account_into, load_account};

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn payment_checks_then_moves_value_and_emits_outbox() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
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
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();

        let mut tx = DrcPaymentTx::unsigned(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(400),
            Amount::from_base_units(7),
            42,
            Hash([9; 32]),
            0,
        );
        sign_drc_payment_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let root_before = drc_payment_root(&store).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let receipt = apply_drc_payment(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        assert_eq!(receipt.fee_paid, 7);
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &alice.address())
                .unwrap()
                .balance,
            593
        );
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &merchant.address())
                .unwrap()
                .balance,
            400
        );
        let event = load_drc_outbox_event(&store, &receipt.payment_id)
            .unwrap()
            .unwrap();
        assert_eq!(event.destination_tag, 42);
        assert_eq!(event.invoice_id, Hash([9; 32]));
        assert_ne!(drc_payment_root(&store).unwrap(), root_before);
    }

    #[test]
    fn duplicate_invoice_rejected_before_mutation() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([2; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut funding = WriteBatch::new();
        credit_account_into(
            &mut funding,
            &store,
            NativeAssetId::DRC,
            &alice.address(),
            Amount::from_base_units(1_000),
        )
        .unwrap();
        store.write_batch(funding).unwrap();
        let invoice = Hash([8; 32]);
        let mut first = DrcPaymentTx::unsigned(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(100),
            Amount::from_base_units(1),
            0,
            invoice,
            0,
        );
        sign_drc_payment_bound(&mut first, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &first, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut duplicate = DrcPaymentTx::unsigned(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(50),
            Amount::from_base_units(1),
            0,
            invoice,
            1,
        );
        sign_drc_payment_bound(&mut duplicate, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let before = load_account(&store, NativeAssetId::DRC, &alice.address()).unwrap();
        let mut rejected_batch = WriteBatch::new();
        let mut rejected_journal = AccountJournal::default();
        assert!(apply_drc_payment(
            &store,
            &duplicate,
            &auth,
            &mut rejected_batch,
            &mut rejected_journal
        )
        .is_err());
        assert!(rejected_batch.is_empty());
        assert!(rejected_journal.before.is_empty());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &alice.address()).unwrap(),
            before
        );
    }
}
