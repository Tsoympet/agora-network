//! Native DRC payment transition and deterministic transport outbox.
//!
//! This is an L1 account payment module. It does not import the historical
//! district-chain ledger, PoW, bridge attestors, or transport cryptography.

use crate::drc_account_auth::verify_drc_payment_operation;
use agora_types::{DrcPaymentOutboxEvent, DrcPaymentReceipt, DrcPaymentTx, Hash, NativeAssetId};
use borsh::BorshDeserialize;

use crate::accounts::{load_account, put_account_into, AccountJournal};
use crate::apply::TxAuthContext;
use crate::columns::ColumnFamily;
use crate::drc_deposit_preauth::load_drc_deposit_preauth;
use crate::drc_policy::load_drc_account_policy;
use crate::store::WriteBatch;
use crate::{StateError, StateStore};

const SEEN_PREFIX: &[u8] = b"payment/drc/seen/";
const INVOICE_PREFIX: &[u8] = b"payment/drc/invoice/";
const OUTBOX_PREFIX: &[u8] = b"payment/drc/outbox/";
const RECEIPT_PREFIX: &[u8] = b"payment/drc/receipt/";
const PAYMENT_ROOT_KEY: &[u8] = b"payment/drc/root";
const PAYMENT_ROOT_DOMAIN: &[u8] = b"agora-drc-payment-root-v4";
pub const DRC_PAYMENT_LEGACY_VERSION: u32 = agora_types::DRC_PAYMENT_LEGACY_VERSION;
pub const DRC_PAYMENT_SOURCE_TAG_VERSION: u32 = agora_types::DRC_PAYMENT_SOURCE_TAG_VERSION;
pub const DRC_PAYMENT_DESTINATION_TAG_VERSION: u32 =
    agora_types::DRC_PAYMENT_DESTINATION_TAG_VERSION;
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

/// Resolve the canonical recipient-scoped invoice index to an exact-delivery receipt.
///
/// The index is derived from root-committed receipt fields, so every component is
/// re-checked before returning data. This prevents a corrupt index from crossing
/// recipient boundaries or exposing an unrelated payment.
pub fn load_drc_payment_by_invoice(
    store: &StateStore,
    recipient: &agora_types::Address,
    invoice_id: &Hash,
) -> Result<Option<DrcPaymentReceipt>, StateError> {
    if *invoice_id == Hash::ZERO {
        return Ok(None);
    }
    let Some(bytes) = store.get_cf(
        ColumnFamily::Meta,
        &payment_invoice_key(recipient, invoice_id),
    )?
    else {
        return Ok(None);
    };
    if bytes.len() != 32 {
        return Err(StateError::Storage(
            "invalid DRC payment invoice index value length".into(),
        ));
    }
    let mut payment_id = [0u8; 32];
    payment_id.copy_from_slice(&bytes);
    let receipt = load_drc_payment_receipt(store, &Hash(payment_id))?.ok_or_else(|| {
        StateError::Storage("DRC payment invoice index points to a missing receipt".into())
    })?;
    if receipt.to != *recipient || receipt.invoice_id != *invoice_id {
        return Err(StateError::Storage(
            "DRC payment invoice index does not match receipt routing".into(),
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
    apply_drc_payment_with_blue_score(store, tx, auth, None, batch, journal)
}

/// Apply a payment under its containing block's consensus-derived GHOSTDAG blue score.
pub fn apply_drc_payment_at_blue_score(
    store: &StateStore,
    tx: &DrcPaymentTx,
    auth: &TxAuthContext,
    application_blue_score: u64,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcPaymentReceipt, StateError> {
    apply_drc_payment_with_blue_score(
        store,
        tx,
        auth,
        Some(application_blue_score),
        batch,
        journal,
    )
}

pub(crate) fn apply_drc_payment_with_blue_score(
    store: &StateStore,
    tx: &DrcPaymentTx,
    auth: &TxAuthContext,
    application_blue_score: Option<u64>,
    batch: &mut WriteBatch,
    journal: &mut AccountJournal,
) -> Result<DrcPaymentReceipt, StateError> {
    tx.validate_envelope_version()
        .map_err(|err| StateError::InvalidTx(err.to_string()))?;
    if tx.amount.as_base_units() == 0 {
        return Err(StateError::InvalidTx("zero DRC payment".into()));
    }
    verify_drc_payment_operation(store, tx, auth)?;
    if tx.version == DRC_PAYMENT_VERSION {
        let score = application_blue_score.ok_or_else(|| {
            StateError::InvalidTx(
                "DRC payment v4 requires a consensus application blue score".into(),
            )
        })?;
        if let Some(cutoff) = tx.last_valid_blue_score {
            if score > cutoff {
                return Err(StateError::InvalidTx(format!(
                    "expired DRC payment: application blue score {score} exceeds last valid blue score {cutoff}"
                )));
            }
        }
    }

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
    let recipient_policy = load_drc_account_policy(store, &tx.to)?;
    if recipient_policy.require_destination_tag && tx.authenticated_destination_tag().is_none() {
        return Err(StateError::InvalidTx(
            "DRC destination tag required by recipient policy".into(),
        ));
    }
    if recipient_policy.deposit_auth_required
        && tx.from != tx.to
        && !load_drc_deposit_preauth(store, &tx.to, &tx.from)?
    {
        return Err(StateError::InvalidTx(
            "DRC deposit authorization required by recipient policy".into(),
        ));
    }

    let mut from = load_account(store, NativeAssetId::DRC, &tx.from)?;
    let mut to = if tx.from == tx.to {
        None
    } else {
        Some(load_account(store, NativeAssetId::DRC, &tx.to)?)
    };
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
        .as_ref()
        .map(|recipient| {
            recipient
                .balance
                .checked_add(tx.amount.as_base_units())
                .ok_or_else(|| StateError::InvalidTx("DRC payment recipient overflow".into()))
        })
        .transpose()?;
    let next_nonce = from
        .nonce
        .checked_add(1)
        .ok_or_else(|| StateError::InvalidTx("DRC payment nonce overflow".into()))?;
    let next_sender_balance = if tx.from == tx.to {
        // XRPL DepositAuth treats self-payments as authorized. Exact native DRC
        // self-payment has a fee-only net effect, but still requires the sender
        // to fund amount + fee before the amount is credited back.
        from.balance
            .checked_sub(debit)
            .and_then(|balance| balance.checked_add(tx.amount.as_base_units()))
            .ok_or_else(|| StateError::InvalidTx("DRC self-payment balance overflow".into()))?
    } else {
        from.balance - debit
    };
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
    if let Some(recipient) = to.as_ref() {
        journal
            .before
            .push((NativeAssetId::DRC, tx.to, recipient.clone()));
    }
    from.balance = next_sender_balance;
    from.nonce = next_nonce;
    put_account_into(batch, NativeAssetId::DRC, &tx.from, &from)?;
    if let (Some(recipient), Some(balance)) = (to.as_mut(), recipient_balance) {
        recipient.balance = balance;
        put_account_into(batch, NativeAssetId::DRC, &tx.to, recipient)?;
    }
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
    use agora_types::{Amount, DrcPaymentResult, DrcPaymentTx, DRC_PAYMENT_RECEIPT_LEGACY_VERSION};

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
        assert_eq!(receipt.version, DRC_PAYMENT_RECEIPT_LEGACY_VERSION);
        assert_eq!(receipt.result, DrcPaymentResult::DeliveredExact);
        assert_eq!(receipt.source_tag, None);
        assert_eq!(receipt.destination_tag, Some(42));
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
        assert_eq!(event.destination_tag, Some(42));
        assert_eq!(event.invoice_id, Hash([9; 32]));
        assert_eq!(
            load_drc_payment_receipt(&store, &receipt.payment_id)
                .unwrap()
                .unwrap(),
            receipt
        );
        assert_eq!(
            load_drc_payment_by_invoice(&store, &merchant.address(), &Hash([9; 32]))
                .unwrap()
                .unwrap(),
            receipt
        );
        assert_eq!(
            drc_payment_root(&store).unwrap(),
            Hash::hash_borsh(&(PAYMENT_ROOT_DOMAIN, root_before, &event, &receipt))
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
        agora_crypto::verify_drc_payment_bound(&tx, &auth.chain_id, &auth.genesis).unwrap();
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

        assert_eq!(
            untagged_receipt.payment_version,
            DRC_PAYMENT_SOURCE_TAG_VERSION
        );
        assert_eq!(untagged_receipt.source_tag, None);
        assert_eq!(untagged_event.source_tag, None);
        assert_eq!(tagged_receipt.source_tag, Some(0));
        assert_eq!(tagged_event.source_tag, Some(0));
        assert_eq!(tagged_event.destination_tag, Some(42));
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

    #[test]
    fn invoice_lookup_is_recipient_scoped_deterministic_and_fail_closed() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let first_merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let second_merchant = derive_bip44(&seed, &Bip44Path::external(2)).unwrap();
        let other_merchant = derive_bip44(&seed, &Bip44Path::external(3)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([7; 32]),
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
        let mut receipts = Vec::new();
        for (nonce, merchant, source_tag, destination_tag) in [
            (0, &first_merchant, Some(11), 21),
            (1, &second_merchant, Some(12), 22),
        ] {
            let mut tx = DrcPaymentTx::unsigned_v2(
                alice.address(),
                merchant.address(),
                Amount::from_base_units(100),
                Amount::from_base_units(1),
                destination_tag,
                source_tag,
                invoice,
                nonce,
            );
            sign_drc_payment_bound(&mut tx, &alice, &auth.chain_id, &auth.genesis).unwrap();
            let mut batch = WriteBatch::new();
            let mut journal = AccountJournal::default();
            let receipt = apply_drc_payment(&store, &tx, &auth, &mut batch, &mut journal).unwrap();
            store.write_batch(batch).unwrap();
            receipts.push(receipt);
        }

        for (merchant, expected) in [
            (&first_merchant, &receipts[0]),
            (&second_merchant, &receipts[1]),
        ] {
            let first = load_drc_payment_by_invoice(&store, &merchant.address(), &invoice)
                .unwrap()
                .unwrap();
            let second = load_drc_payment_by_invoice(&store, &merchant.address(), &invoice)
                .unwrap()
                .unwrap();
            assert_eq!(&first, expected);
            assert_eq!(second, first);
        }
        assert_ne!(receipts[0].payment_id, receipts[1].payment_id);
        assert_eq!(receipts[0].source_tag, Some(11));
        assert_eq!(receipts[0].destination_tag, Some(21));
        assert_eq!(receipts[1].source_tag, Some(12));
        assert_eq!(receipts[1].destination_tag, Some(22));
        assert!(
            load_drc_payment_by_invoice(&store, &other_merchant.address(), &invoice)
                .unwrap()
                .is_none()
        );
        assert!(
            load_drc_payment_by_invoice(&store, &first_merchant.address(), &Hash([7; 32]))
                .unwrap()
                .is_none()
        );
        assert!(
            load_drc_payment_by_invoice(&store, &first_merchant.address(), &Hash::ZERO)
                .unwrap()
                .is_none()
        );

        store
            .put_cf(
                ColumnFamily::Meta,
                &payment_invoice_key(&other_merchant.address(), &invoice),
                &[1; 31],
            )
            .unwrap();
        let error = load_drc_payment_by_invoice(&store, &other_merchant.address(), &invoice)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invoice index value length"));

        store
            .put_cf(
                ColumnFamily::Meta,
                &payment_invoice_key(&other_merchant.address(), &invoice),
                Hash([6; 32]).as_bytes(),
            )
            .unwrap();
        let error = load_drc_payment_by_invoice(&store, &other_merchant.address(), &invoice)
            .unwrap_err()
            .to_string();
        assert!(error.contains("points to a missing receipt"));

        store
            .put_cf(
                ColumnFamily::Meta,
                &payment_invoice_key(&other_merchant.address(), &invoice),
                receipts[0].payment_id.as_bytes(),
            )
            .unwrap();
        let error = load_drc_payment_by_invoice(&store, &other_merchant.address(), &invoice)
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not match receipt routing"));
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
        assert_eq!(
            load_drc_payment_by_invoice(&reopened, &merchant.address(), &Hash([9; 32]))
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
        unsupported.version = agora_types::DRC_PAYMENT_VERSION + 1;

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
    fn recipient_policy_enforces_presence_accepts_zero_and_leaves_outgoing_unaffected() {
        use agora_crypto::sign_drc_account_policy_bound;
        use agora_types::DrcAccountPolicyTx;

        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([4; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut funding = WriteBatch::new();
        for (owner, amount) in [(&alice, 1_000), (&merchant, 100)] {
            credit_account_into(
                &mut funding,
                &store,
                NativeAssetId::DRC,
                &owner.address(),
                Amount::from_base_units(amount),
            )
            .unwrap();
        }
        store.write_batch(funding).unwrap();

        let mut set =
            DrcAccountPolicyTx::set_require_destination_tag(merchant.address(), Amount::ZERO, 0);
        sign_drc_account_policy_bound(&mut set, &merchant, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_account_policy(&store, &set, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut missing = DrcPaymentTx::unsigned_v3(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(10),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
        );
        sign_drc_payment_bound(&mut missing, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut rejected = WriteBatch::new();
        let mut rejected_journal = AccountJournal::default();
        let error = apply_drc_payment(
            &store,
            &missing,
            &auth,
            &mut rejected,
            &mut rejected_journal,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("destination tag required"), "{error}");
        assert!(rejected.is_empty());
        assert!(rejected_journal.before.is_empty());

        let mut tagged_zero = DrcPaymentTx::unsigned_v3(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(10),
            Amount::ZERO,
            Some(0),
            None,
            Hash::ZERO,
            0,
        );
        sign_drc_payment_bound(&mut tagged_zero, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        let receipt =
            apply_drc_payment(&store, &tagged_zero, &auth, &mut batch, &mut journal).unwrap();
        assert_eq!(receipt.destination_tag, Some(0));
        store.write_batch(batch).unwrap();

        let mut outgoing = DrcPaymentTx::unsigned_v3(
            merchant.address(),
            alice.address(),
            Amount::from_base_units(5),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            1,
        );
        sign_drc_payment_bound(&mut outgoing, &merchant, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &outgoing, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut clear =
            DrcAccountPolicyTx::clear_require_destination_tag(merchant.address(), Amount::ZERO, 2);
        sign_drc_account_policy_bound(&mut clear, &merchant, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_account_policy(&store, &clear, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut untagged_after_clear = DrcPaymentTx::unsigned_v3(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(10),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            1,
        );
        sign_drc_payment_bound(
            &mut untagged_after_clear,
            &alice,
            &auth.chain_id,
            &auth.genesis,
        )
        .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(
            &store,
            &untagged_after_clear,
            &auth,
            &mut batch,
            &mut journal,
        )
        .unwrap();
    }

    #[test]
    fn deposit_auth_composes_with_self_payments_tags_and_dormant_records() {
        use agora_crypto::{sign_drc_account_policy_bound, sign_drc_deposit_preauth_bound};
        use agora_types::{DrcAccountPolicyTx, DrcDepositPreauthTx};

        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let authorized = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let owner = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let unauthorized = derive_bip44(&seed, &Bip44Path::external(2)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([7; 32]),
            data_availability_network_fingerprint: None,
        };
        let mut funding = WriteBatch::new();
        for account in [&authorized, &owner, &unauthorized] {
            credit_account_into(
                &mut funding,
                &store,
                NativeAssetId::DRC,
                &account.address(),
                Amount::from_base_units(100),
            )
            .unwrap();
        }
        store.write_batch(funding).unwrap();

        let mut enable =
            DrcAccountPolicyTx::set_deposit_auth_required(owner.address(), Amount::ZERO, 0);
        sign_drc_account_policy_bound(&mut enable, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_account_policy(&store, &enable, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut grant =
            DrcDepositPreauthTx::authorize(owner.address(), authorized.address(), Amount::ZERO, 1);
        sign_drc_deposit_preauth_bound(&mut grant, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_deposit_preauth(&store, &grant, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut rejected = DrcPaymentTx::unsigned_v3(
            unauthorized.address(),
            owner.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );
        sign_drc_payment_bound(&mut rejected, &unauthorized, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut rejected_batch = WriteBatch::new();
        let mut rejected_journal = AccountJournal::default();
        let error = apply_drc_payment(
            &store,
            &rejected,
            &auth,
            &mut rejected_batch,
            &mut rejected_journal,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("deposit authorization required"), "{error}");
        assert!(rejected_batch.is_empty());
        assert!(rejected_journal.before.is_empty());

        let mut accepted = DrcPaymentTx::unsigned_v3(
            authorized.address(),
            owner.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
        );
        sign_drc_payment_bound(&mut accepted, &authorized, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &accepted, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut self_payment = DrcPaymentTx::unsigned_v3(
            owner.address(),
            owner.address(),
            Amount::from_base_units(50),
            Amount::from_base_units(2),
            None,
            None,
            Hash::ZERO,
            2,
        );
        sign_drc_payment_bound(&mut self_payment, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let owner_before = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &self_payment, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        let owner_after = load_account(&store, NativeAssetId::DRC, &owner.address()).unwrap();
        assert_eq!(owner_after.balance, owner_before.balance - 2);
        assert_eq!(owner_after.nonce, owner_before.nonce + 1);

        let mut disable =
            DrcAccountPolicyTx::clear_deposit_auth_required(owner.address(), Amount::ZERO, 3);
        sign_drc_account_policy_bound(&mut disable, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_account_policy(&store, &disable, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();
        assert!(
            crate::load_drc_deposit_preauth(&store, &owner.address(), &authorized.address())
                .unwrap(),
            "clearing DepositAuth leaves dormant address records"
        );

        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &rejected, &auth, &mut batch, &mut journal).unwrap();
        store.write_batch(batch).unwrap();

        let mut require_tag =
            DrcAccountPolicyTx::set_require_destination_tag(owner.address(), Amount::ZERO, 4);
        sign_drc_account_policy_bound(&mut require_tag, &owner, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        crate::apply_drc_account_policy(&store, &require_tag, &auth, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();

        let mut missing_tag = DrcPaymentTx::unsigned_v3(
            owner.address(),
            owner.address(),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            5,
        );
        sign_drc_payment_bound(&mut missing_tag, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let mut rejected_batch = WriteBatch::new();
        let mut rejected_journal = AccountJournal::default();
        assert!(apply_drc_payment(
            &store,
            &missing_tag,
            &auth,
            &mut rejected_batch,
            &mut rejected_journal,
        )
        .is_err());
        assert!(rejected_batch.is_empty());

        missing_tag.destination_tag = Some(0);
        sign_drc_payment_bound(&mut missing_tag, &owner, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment(&store, &missing_tag, &auth, &mut batch, &mut journal).unwrap();
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

    #[test]
    fn payment_v4_expiry_is_inclusive_atomic_and_preserves_nonce() {
        let store = StateStore::open_in_memory();
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let alice = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let merchant = derive_bip44(&seed, &Bip44Path::external(1)).unwrap();
        let auth = TxAuthContext {
            chain_id: "agora-dev".into(),
            genesis: Hash([11; 32]),
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

        let mut at_cutoff = DrcPaymentTx::unsigned_v4(
            alice.address(),
            merchant.address(),
            Amount::from_base_units(10),
            Amount::from_base_units(1),
            None,
            None,
            Hash::ZERO,
            0,
            Some(5),
        );
        sign_drc_payment_bound(&mut at_cutoff, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment_at_blue_score(&store, &at_cutoff, &auth, 5, &mut batch, &mut journal)
            .unwrap();
        store.write_batch(batch).unwrap();

        let mut after_cutoff = at_cutoff.clone();
        after_cutoff.amount = Amount::from_base_units(11);
        sign_drc_payment_bound(&mut after_cutoff, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let before = load_account(&store, NativeAssetId::DRC, &alice.address()).unwrap();
        let root_before = drc_payment_root(&store).unwrap();
        let mut rejected_batch = WriteBatch::new();
        let mut rejected_journal = AccountJournal::default();
        let error = apply_drc_payment_at_blue_score(
            &store,
            &after_cutoff,
            &auth,
            6,
            &mut rejected_batch,
            &mut rejected_journal,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("expired DRC payment"), "{error}");
        assert!(rejected_batch.is_empty());
        assert!(rejected_journal.before.is_empty());
        assert_eq!(
            load_account(&store, NativeAssetId::DRC, &alice.address()).unwrap(),
            before
        );
        assert_eq!(drc_payment_root(&store).unwrap(), root_before);

        let mut retry = at_cutoff.clone();
        retry.nonce = 1;
        sign_drc_payment_bound(&mut retry, &alice, &auth.chain_id, &auth.genesis).unwrap();
        let mut batch = WriteBatch::new();
        let mut journal = AccountJournal::default();
        apply_drc_payment_at_blue_score(&store, &retry, &auth, 5, &mut batch, &mut journal)
            .unwrap();
    }
}
