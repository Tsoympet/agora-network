//! Native DRC payment transition and deterministic transport outbox.
//!
//! This is an L1 account payment module. It does not import the historical
//! district-chain ledger, PoW, bridge attestors, or transport cryptography.

use agora_crypto::verify_drc_payment_bound;
use agora_types::{DrcPaymentOutboxEvent, DrcPaymentReceipt, DrcPaymentTx, Hash, NativeAssetId};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const SEEN_PREFIX: &[u8] = b"payment/drc/seen/";
const INVOICE_PREFIX: &[u8] = b"payment/drc/invoice/";
const OUTBOX_PREFIX: &[u8] = b"payment/drc/outbox/";
const RECEIPT_PREFIX: &[u8] = b"payment/drc/receipt/";
const PAYMENT_ROOT_KEY: &[u8] = b"payment/drc/root";
const PAYMENT_ROOT_DOMAIN: &[u8] = b"agora-drc-payment-root-v2";
pub const DRC_PAYMENT_LEGACY_VERSION: u32 = agora_types::DRC_PAYMENT_LEGACY_VERSION;
pub const DRC_PAYMENT_VERSION: u32 = agora_types::DRC_PAYMENT_VERSION;

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

pub fn payment_receipt_key(payment_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(RECEIPT_PREFIX.len() + 32);
    key.extend_from_slice(RECEIPT_PREFIX);
    key.extend_from_slice(payment_id.as_bytes());
    key
}

/// Meta keys changed by an accepted payment, for reorg snapshots.
pub fn payment_meta_keys(tx: &DrcPaymentTx) -> Vec<Vec<u8>> {
    let id = tx.payment_id();
    let mut keys = vec![
        payment_seen_key(&id),
        payment_outbox_key(&id),
        payment_receipt_key(&id),
        PAYMENT_ROOT_KEY.to_vec(),
    ];
    if tx.invoice_id != Hash::ZERO {
        keys.push(payment_invoice_key(&tx.to, &tx.invoice_id));
    }
    keys
}

pub fn load_drc_payment_receipt(
    store: &StateStore,
    payment_id: &Hash,
) -> Result<Option<DrcPaymentReceipt>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &payment_receipt_key(payment_id))? else {
        return Ok(None);
    };
    let receipt = DrcPaymentReceipt::try_from_slice(&bytes)
        .map_err(|e| StateError::Storage(e.to_string()))?;
    receipt
        .validate_exact()
        .map_err(|e| StateError::Storage(e.to_string()))?;
    if receipt.payment_id != *payment_id {
        return Err(StateError::Storage(
            "DRC payment receipt id does not match index key".into(),
        ));
    }
    Ok(Some(receipt))
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
        return Ok(Hash::hash_borsh(&(PAYMENT_ROOT_DOMAIN, Hash::ZERO)));
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
    tx.validate_envelope_version()
        .map_err(|err| StateError::InvalidTx(err.to_string()))?;
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
    let receipt = DrcPaymentReceipt::delivered_exact(tx);
    let event_bytes = borsh::to_vec(&event).map_err(|e| StateError::Storage(e.to_string()))?;
    let receipt_bytes = borsh::to_vec(&receipt).map_err(|e| StateError::Storage(e.to_string()))?;
    let prior_payment_root = drc_payment_root(store)?;
    let next_payment_root =
        Hash::hash_borsh(&(PAYMENT_ROOT_DOMAIN, prior_payment_root, &event, &receipt));

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
        &payment_receipt_key(&payment_id),
        &receipt_bytes,
    );
    batch.put_cf(
        ColumnFamily::Meta,
        PAYMENT_ROOT_KEY,
        next_payment_root.as_bytes(),
    );

    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_drc_payment_bound, Bip44Path};
    use agora_types::{Amount, DrcPaymentResult, DrcPaymentTx, DRC_PAYMENT_RECEIPT_VERSION};

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

        assert_eq!(receipt.fee_paid, Amount::from_base_units(7));
        assert_eq!(receipt.requested_amount, Amount::from_base_units(400));
        assert_eq!(receipt.delivered_amount, receipt.requested_amount);
        assert_eq!(receipt.payment_version, DRC_PAYMENT_LEGACY_VERSION);
        assert_eq!(receipt.version, DRC_PAYMENT_RECEIPT_VERSION);
        assert_eq!(receipt.result, DrcPaymentResult::DeliveredExact);
        assert_eq!(receipt.source_tag, None);
        assert_eq!(receipt.destination_tag, 42);
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
        assert_eq!(event.payment_version, DRC_PAYMENT_LEGACY_VERSION);
        assert_eq!(event.source_tag, None);
        assert_eq!(event.destination_tag, 42);
        assert_eq!(event.invoice_id, Hash([9; 32]));
        assert_eq!(
            load_drc_payment_receipt(&store, &receipt.payment_id)
                .unwrap()
                .unwrap(),
            receipt
        );
        assert_eq!(list_drc_outbox(&store, 1).unwrap(), vec![event]);
        assert_ne!(drc_payment_root(&store).unwrap(), root_before);
    }

    fn settle_v2(
        source_tag: Option<u32>,
    ) -> (DrcPaymentReceipt, DrcPaymentOutboxEvent, Hash, Hash) {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([3; 32]),
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

        let mut tx = DrcPaymentTx::unsigned_v2(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(400),
            Amount::from_base_units(7),
            42,
            source_tag,
            Hash([9; 32]),
            0,
        );
        sign_drc_payment_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let signed_bytes = borsh::to_vec(&tx).unwrap();
        let tx = DrcPaymentTx::try_from_slice(&signed_bytes).unwrap();
        verify_drc_payment_bound(&tx, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let receipt = apply_drc_payment(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

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
        assert_eq!(
            load_drc_payment_receipt(&store, &receipt.payment_id)
                .unwrap()
                .unwrap(),
            receipt
        );
        let payment_root = drc_payment_root(&store).unwrap();
        let state_root = crate::compose_trident_state_root(&store, &Hash([4; 32])).unwrap();
        (receipt, event, payment_root, state_root)
    }

    #[test]
    fn source_tag_survives_settlement_and_changes_payment_and_state_roots() {
        let (untagged_receipt, untagged_event, untagged_payment_root, untagged_state_root) =
            settle_v2(None);
        let (tagged_receipt, tagged_event, tagged_payment_root, tagged_state_root) =
            settle_v2(Some(0));

        assert_eq!(untagged_receipt.payment_version, DRC_PAYMENT_VERSION);
        assert_eq!(untagged_receipt.source_tag, None);
        assert_eq!(untagged_event.source_tag, None);
        assert_eq!(tagged_receipt.source_tag, Some(0));
        assert_eq!(tagged_event.source_tag, Some(0));
        assert_eq!(tagged_event.destination_tag, 42);
        assert_ne!(tagged_receipt.payment_id, untagged_receipt.payment_id);
        assert_ne!(tagged_payment_root, untagged_payment_root);
        assert_ne!(tagged_state_root, untagged_state_root);
    }

    #[test]
    fn exact_receipt_root_is_deterministic_and_unknown_id_is_none() {
        let (first_receipt, _, first_payment_root, first_state_root) = settle_v2(Some(u32::MAX));
        let (second_receipt, _, second_payment_root, second_state_root) = settle_v2(Some(u32::MAX));

        assert_eq!(first_receipt, second_receipt);
        assert_eq!(
            first_receipt.requested_amount,
            first_receipt.delivered_amount
        );
        assert_eq!(first_payment_root, second_payment_root);
        assert_eq!(first_state_root, second_state_root);
        assert!(
            load_drc_payment_receipt(&StateStore::open_in_memory(), &Hash::ZERO)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn malformed_partial_or_misindexed_receipt_fails_closed() {
        let store = StateStore::open_in_memory();
        let tx = DrcPaymentTx::unsigned_v2(
            agora_types::Address([1; 20]),
            agora_types::Address([2; 20]),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            3,
            Some(4),
            Hash([5; 32]),
            0,
        );
        let mut malformed = DrcPaymentReceipt::delivered_exact(&tx);
        malformed.delivered_amount = Amount::from_base_units(9);
        let id = malformed.payment_id;
        store
            .put_cf(
                ColumnFamily::Meta,
                &payment_receipt_key(&id),
                &borsh::to_vec(&malformed).unwrap(),
            )
            .unwrap();
        let error = load_drc_payment_receipt(&store, &id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("requested and delivered amounts differ"));

        malformed.delivered_amount = malformed.requested_amount;
        let wrong_id = Hash([9; 32]);
        store
            .put_cf(
                ColumnFamily::Meta,
                &payment_receipt_key(&wrong_id),
                &borsh::to_vec(&malformed).unwrap(),
            )
            .unwrap();
        let error = load_drc_payment_receipt(&store, &wrong_id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not match index key"));
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn exact_delivery_receipt_persists_across_rocksdb_reopen() {
        let directory = std::env::temp_dir().join(format!(
            "agora-drc-receipt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&directory);

        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([6; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut tx = DrcPaymentTx::unsigned_v2(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(25),
            Amount::from_base_units(1),
            7,
            Some(8),
            Hash([9; 32]),
            0,
        );
        sign_drc_payment_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let expected = {
            let store = StateStore::open(&directory).unwrap();
            let mut funding = WriteBatch::new();
            credit_account_into(
                &mut funding,
                &store,
                NativeAssetId::DRC,
                &alice.address(),
                Amount::from_base_units(100),
            )
            .unwrap();
            store.write_batch(funding).unwrap();
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            let receipt = apply_drc_payment(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
            store.write_batch(batch).unwrap();
            receipt
        };

        let reopened = StateStore::open(&directory).unwrap();
        assert_eq!(
            load_drc_payment_receipt(&reopened, &expected.payment_id)
                .unwrap()
                .unwrap(),
            expected
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_or_unsupported_payment_versions_stage_no_state() {
        let store = StateStore::open_in_memory();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([5; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut unsupported = DrcPaymentTx::unsigned_v2(
            agora_types::Address([1; 20]),
            agora_types::Address([2; 20]),
            Amount::from_base_units(1),
            Amount::ZERO,
            0,
            None,
            Hash::ZERO,
            0,
        );
        unsupported.version += 1;

        let mut legacy_with_source = DrcPaymentTx::unsigned(
            agora_types::Address([1; 20]),
            agora_types::Address([2; 20]),
            Amount::from_base_units(1),
            Amount::ZERO,
            0,
            Hash::ZERO,
            0,
        );
        legacy_with_source.source_tag = Some(0);

        for (tx, expected) in [
            (unsupported, "unsupported DRC payment version"),
            (legacy_with_source, "v1 cannot carry a source tag"),
        ] {
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            let err = apply_drc_payment(&store, &tx, &auth, &mut batch, &mut journal)
                .unwrap_err()
                .to_string();
            assert!(err.contains(expected), "{err}");
            assert!(batch.is_empty());
            assert!(journal.before.is_empty());
        }
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
