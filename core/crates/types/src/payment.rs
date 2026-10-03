//! Signed DRC payment envelopes and outbox events for Trident L1.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{Address, Amount, Hash};

/// Frozen legacy payment envelope version.
pub const DRC_PAYMENT_LEGACY_VERSION: u32 = 1;
/// Current payment envelope version; v2 adds an authenticated optional source tag.
pub const DRC_PAYMENT_VERSION: u32 = 2;
/// Current durable exact-delivery receipt version.
pub const DRC_PAYMENT_RECEIPT_VERSION: u32 = 1;
/// Frozen domain separator for v1 network-bound DRC payment signatures.
pub const DRC_PAYMENT_V1_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-payment-v1";
/// Domain separator for v2 network-bound DRC payment signatures.
pub const DRC_PAYMENT_V2_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-payment-v2";
/// Backward-compatible name for the frozen v1 signing domain.
pub const DRC_PAYMENT_SIGNING_DOMAIN: &[u8] = DRC_PAYMENT_V1_SIGNING_DOMAIN;

/// Signed account-based DRC payment.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
pub struct DrcPaymentTx {
    pub version: u32,
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    pub fee: Amount,
    /// `0` indicates that the destination does not require a tag.
    pub destination_tag: u32,
    /// Optional sender-local routing metadata. `Some(0)` is distinct from no source tag.
    #[serde(default)]
    pub source_tag: Option<u32>,
    /// `Hash::ZERO` indicates that the payment is not associated with an invoice.
    pub invoice_id: Hash,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl DrcPaymentTx {
    pub fn validate_envelope_version(&self) -> Result<(), DrcPaymentEnvelopeError> {
        match self.version {
            DRC_PAYMENT_LEGACY_VERSION if self.source_tag.is_some() => {
                Err(DrcPaymentEnvelopeError::LegacySourceTag)
            }
            DRC_PAYMENT_LEGACY_VERSION | DRC_PAYMENT_VERSION => Ok(()),
            version => Err(DrcPaymentEnvelopeError::UnsupportedVersion(version)),
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version == DRC_PAYMENT_LEGACY_VERSION {
            return borsh::to_vec(&(
                DRC_PAYMENT_V1_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                self.version,
                self.from,
                self.to,
                self.amount,
                self.fee,
                self.destination_tag,
                self.invoice_id,
                self.nonce,
            ))
            .expect("borsh serialize DRC payment v1 body");
        }

        borsh::to_vec(&(
            DRC_PAYMENT_V2_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.from,
            self.to,
            self.amount,
            self.fee,
            self.destination_tag,
            self.source_tag,
            self.invoice_id,
            self.nonce,
        ))
        .expect("borsh serialize DRC payment v2 body")
    }

    /// Hashes the complete signed envelope, including authorization material.
    pub fn payment_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    /// Construct a frozen v1 payment without source-tag support.
    #[allow(clippy::too_many_arguments)]
    pub fn unsigned(
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
        destination_tag: u32,
        invoice_id: Hash,
        nonce: u64,
    ) -> Self {
        Self {
            version: DRC_PAYMENT_LEGACY_VERSION,
            from,
            to,
            amount,
            fee,
            destination_tag,
            source_tag: None,
            invoice_id,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }

    /// Construct a v2 payment with an authenticated optional source tag.
    #[allow(clippy::too_many_arguments)]
    pub fn unsigned_v2(
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
        destination_tag: u32,
        source_tag: Option<u32>,
        invoice_id: Hash,
        nonce: u64,
    ) -> Self {
        Self {
            version: DRC_PAYMENT_VERSION,
            from,
            to,
            amount,
            fee,
            destination_tag,
            source_tag,
            invoice_id,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }
}

/// A v2-only field may never be smuggled into the frozen v1 signing layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcPaymentEnvelopeError {
    #[error("unsupported DRC payment version {0}")]
    UnsupportedVersion(u32),
    #[error("DRC payment v1 cannot carry a source tag")]
    LegacySourceTag,
}

impl BorshSerialize for DrcPaymentTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.from, writer)?;
        BorshSerialize::serialize(&self.to, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.destination_tag, writer)?;
        if self.version != DRC_PAYMENT_LEGACY_VERSION {
            BorshSerialize::serialize(&self.source_tag, writer)?;
        }
        BorshSerialize::serialize(&self.invoice_id, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)
    }
}

impl BorshDeserialize for DrcPaymentTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let version = u32::deserialize_reader(reader)?;
        let from = Address::deserialize_reader(reader)?;
        let to = Address::deserialize_reader(reader)?;
        let amount = Amount::deserialize_reader(reader)?;
        let fee = Amount::deserialize_reader(reader)?;
        let destination_tag = u32::deserialize_reader(reader)?;
        let source_tag = if version == DRC_PAYMENT_LEGACY_VERSION {
            None
        } else {
            Option::<u32>::deserialize_reader(reader)?
        };
        Ok(Self {
            version,
            from,
            to,
            amount,
            fee,
            destination_tag,
            source_tag,
            invoice_id: Hash::deserialize_reader(reader)?,
            nonce: u64::deserialize_reader(reader)?,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
        })
    }
}

/// Contract-free DRC settlement has one outcome: the full requested amount arrived.
///
/// A partial-delivery variant is intentionally absent. Adding any other result requires
/// a separately versioned receipt and state transition.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DrcPaymentResult {
    DeliveredExact,
}

impl DrcPaymentResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeliveredExact => "delivered_exact",
        }
    }
}

/// Root-committed settlement receipt indexed by the canonical signed-envelope id.
///
/// This type is separate from [`DrcPaymentOutboxEvent`] so the frozen outbox-v1
/// encoding remains unchanged.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcPaymentReceipt {
    pub version: u32,
    pub payment_id: Hash,
    pub payment_version: u32,
    pub result: DrcPaymentResult,
    pub from: Address,
    pub to: Address,
    pub requested_amount: Amount,
    pub delivered_amount: Amount,
    pub fee_paid: Amount,
    #[serde(default)]
    pub source_tag: Option<u32>,
    pub destination_tag: u32,
    pub invoice_id: Hash,
}

impl DrcPaymentReceipt {
    pub fn delivered_exact(tx: &DrcPaymentTx) -> Self {
        Self {
            version: DRC_PAYMENT_RECEIPT_VERSION,
            payment_id: tx.payment_id(),
            payment_version: tx.version,
            result: DrcPaymentResult::DeliveredExact,
            from: tx.from,
            to: tx.to,
            requested_amount: tx.amount,
            delivered_amount: tx.amount,
            fee_paid: tx.fee,
            source_tag: tx.source_tag,
            destination_tag: tx.destination_tag,
            invoice_id: tx.invoice_id,
        }
    }

    /// Fail closed if stored bytes could imply anything other than full delivery.
    pub fn validate_exact(&self) -> Result<(), DrcPaymentReceiptError> {
        if self.version != DRC_PAYMENT_RECEIPT_VERSION {
            return Err(DrcPaymentReceiptError::UnsupportedVersion(self.version));
        }
        if !matches!(self.result, DrcPaymentResult::DeliveredExact) {
            return Err(DrcPaymentReceiptError::UnsupportedResult);
        }
        if self.requested_amount == Amount::ZERO {
            return Err(DrcPaymentReceiptError::ZeroAmount);
        }
        if self.requested_amount != self.delivered_amount {
            return Err(DrcPaymentReceiptError::AmountMismatch);
        }
        match self.payment_version {
            DRC_PAYMENT_LEGACY_VERSION if self.source_tag.is_some() => {
                Err(DrcPaymentReceiptError::LegacySourceTag)
            }
            DRC_PAYMENT_LEGACY_VERSION | DRC_PAYMENT_VERSION => Ok(()),
            version => Err(DrcPaymentReceiptError::UnsupportedPaymentVersion(version)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcPaymentReceiptError {
    #[error("unsupported DRC payment receipt version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC payment receipt result")]
    UnsupportedResult,
    #[error("DRC payment receipt amount must be non-zero")]
    ZeroAmount,
    #[error("DRC payment receipt requested and delivered amounts differ")]
    AmountMismatch,
    #[error("unsupported DRC payment envelope version {0} in receipt")]
    UnsupportedPaymentVersion(u32),
    #[error("DRC payment v1 receipt cannot carry a source tag")]
    LegacySourceTag,
}

/// Durable notification emitted after accepting a DRC payment.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrcPaymentOutboxEvent {
    pub payment_id: Hash,
    #[serde(default = "legacy_payment_version")]
    pub payment_version: u32,
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    #[serde(default)]
    pub source_tag: Option<u32>,
    pub destination_tag: u32,
    pub invoice_id: Hash,
}

const fn legacy_payment_version() -> u32 {
    DRC_PAYMENT_LEGACY_VERSION
}

impl DrcPaymentOutboxEvent {
    pub fn from_tx(tx: &DrcPaymentTx) -> Self {
        Self {
            payment_id: tx.payment_id(),
            payment_version: tx.version,
            from: tx.from,
            to: tx.to,
            amount: tx.amount,
            source_tag: tx.source_tag,
            destination_tag: tx.destination_tag,
            invoice_id: tx.invoice_id,
        }
    }
}

impl BorshSerialize for DrcPaymentOutboxEvent {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        // v1 storage bytes are frozen; v2 is an explicit trailing extension.
        BorshSerialize::serialize(&self.payment_id, writer)?;
        BorshSerialize::serialize(&self.from, writer)?;
        BorshSerialize::serialize(&self.to, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.destination_tag, writer)?;
        BorshSerialize::serialize(&self.invoice_id, writer)?;
        if self.payment_version != DRC_PAYMENT_LEGACY_VERSION {
            BorshSerialize::serialize(&self.payment_version, writer)?;
            BorshSerialize::serialize(&self.source_tag, writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for DrcPaymentOutboxEvent {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let payment_id = Hash::deserialize_reader(reader)?;
        let from = Address::deserialize_reader(reader)?;
        let to = Address::deserialize_reader(reader)?;
        let amount = Amount::deserialize_reader(reader)?;
        let destination_tag = u32::deserialize_reader(reader)?;
        let invoice_id = Hash::deserialize_reader(reader)?;
        let payment_version =
            deserialize_optional_u32(reader)?.unwrap_or(DRC_PAYMENT_LEGACY_VERSION);
        let source_tag = match payment_version {
            DRC_PAYMENT_LEGACY_VERSION => None,
            DRC_PAYMENT_VERSION => Option::<u32>::deserialize_reader(reader)?,
            version => {
                return Err(borsh::io::Error::new(
                    borsh::io::ErrorKind::InvalidData,
                    format!("unsupported DRC outbox payment version {version}"),
                ));
            }
        };
        Ok(Self {
            payment_id,
            payment_version,
            from,
            to,
            amount,
            source_tag,
            destination_tag,
            invoice_id,
        })
    }
}

fn deserialize_optional_u32<R: borsh::io::Read>(
    reader: &mut R,
) -> Result<Option<u32>, borsh::io::Error> {
    let mut bytes = [0u8; 4];
    let mut filled = 0usize;
    while filled < bytes.len() {
        match reader.read(&mut bytes[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => {
                return Err(borsh::io::Error::new(
                    borsh::io::ErrorKind::UnexpectedEof,
                    "partial DRC outbox payment version",
                ));
            }
            Ok(read) => filled += read,
            Err(err) if err.kind() == borsh::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(Some(u32::from_le_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(BorshSerialize)]
    struct LegacyDrcPaymentTx {
        version: u32,
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
        destination_tag: u32,
        invoice_id: Hash,
        nonce: u64,
        public_key: Vec<u8>,
        signature: Vec<u8>,
    }

    #[derive(BorshSerialize)]
    struct LegacyDrcPaymentOutboxEvent {
        payment_id: Hash,
        from: Address,
        to: Address,
        amount: Amount,
        destination_tag: u32,
        invoice_id: Hash,
    }

    #[derive(BorshSerialize)]
    struct DrcPaymentReceiptV1 {
        version: u32,
        payment_id: Hash,
        payment_version: u32,
        result: DrcPaymentResult,
        from: Address,
        to: Address,
        requested_amount: Amount,
        delivered_amount: Amount,
        fee_paid: Amount,
        source_tag: Option<u32>,
        destination_tag: u32,
        invoice_id: Hash,
    }

    fn legacy_payment() -> DrcPaymentTx {
        DrcPaymentTx::unsigned(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(4),
            5,
            Hash([6u8; 32]),
            7,
        )
    }

    #[test]
    fn legacy_payment_bytes_and_id_are_frozen() {
        let mut tx = legacy_payment();
        let expected_bytes = borsh::to_vec(&LegacyDrcPaymentTx {
            version: tx.version,
            from: tx.from,
            to: tx.to,
            amount: tx.amount,
            fee: tx.fee,
            destination_tag: tx.destination_tag,
            invoice_id: tx.invoice_id,
            nonce: tx.nonce,
            public_key: tx.public_key.clone(),
            signature: tx.signature.clone(),
        })
        .unwrap();
        assert_eq!(borsh::to_vec(&tx).unwrap(), expected_bytes);
        assert_eq!(DrcPaymentTx::try_from_slice(&expected_bytes).unwrap(), tx);

        let unsigned_id = tx.payment_id();
        assert_eq!(
            unsigned_id,
            Hash::from_hex("7adb199290ae99a6d12830bbdb331ea51330e040b8b61d414473830c4aa18f14")
                .expect("locked DRC payment id")
        );

        tx.signature = vec![8u8; 64];
        assert_ne!(tx.payment_id(), unsigned_id);
    }

    #[test]
    fn v2_source_tag_roundtrips_at_boundaries_and_changes_id() {
        let without_source = DrcPaymentTx::unsigned_v2(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(4),
            5,
            None,
            Hash([6u8; 32]),
            7,
        );
        let without_source_bytes = borsh::to_vec(&without_source).unwrap();
        assert_eq!(
            DrcPaymentTx::try_from_slice(&without_source_bytes).unwrap(),
            without_source
        );

        for source_tag in [0, u32::MAX] {
            let tx = DrcPaymentTx::unsigned_v2(
                Address([1u8; 20]),
                Address([2u8; 20]),
                Amount::from_base_units(3),
                Amount::from_base_units(4),
                5,
                Some(source_tag),
                Hash([6u8; 32]),
                7,
            );
            let bytes = borsh::to_vec(&tx).unwrap();
            assert_eq!(DrcPaymentTx::try_from_slice(&bytes).unwrap(), tx);
            assert_ne!(tx.payment_id(), without_source.payment_id());
            if source_tag == 0 {
                assert_eq!(
                    tx.payment_id(),
                    Hash::from_hex(
                        "e63ef94265d90d4fdf84da444188424af526a0ffebf7d9fae16309e028f54645"
                    )
                    .expect("locked DRC payment v2 id")
                );
            }
        }
    }

    #[test]
    fn malformed_source_tag_and_versions_fail_closed() {
        let mut malformed = borsh::to_vec(&DrcPaymentTx::unsigned_v2(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(4),
            5,
            Some(6),
            Hash([7u8; 32]),
            8,
        ))
        .unwrap();
        malformed[64] = 2;
        assert!(DrcPaymentTx::try_from_slice(&malformed).is_err());

        let mut unsupported = DrcPaymentTx::unsigned_v2(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(4),
            5,
            None,
            Hash([7u8; 32]),
            8,
        );
        unsupported.version = DRC_PAYMENT_VERSION + 1;
        assert_eq!(
            unsupported.validate_envelope_version(),
            Err(DrcPaymentEnvelopeError::UnsupportedVersion(
                DRC_PAYMENT_VERSION + 1
            ))
        );

        let mut legacy_with_source = legacy_payment();
        legacy_with_source.source_tag = Some(0);
        assert_eq!(
            legacy_with_source.validate_envelope_version(),
            Err(DrcPaymentEnvelopeError::LegacySourceTag)
        );
    }

    #[test]
    fn serde_accepts_legacy_json_without_source_tag() {
        let tx = legacy_payment();
        let mut value = serde_json::to_value(&tx).unwrap();
        value.as_object_mut().unwrap().remove("source_tag").unwrap();
        assert_eq!(serde_json::from_value::<DrcPaymentTx>(value).unwrap(), tx);
    }

    #[test]
    fn outbox_event_versions_payment_routing_fields() {
        let tx = DrcPaymentTx::unsigned_v2(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(4),
            5,
            Some(6),
            Hash([7u8; 32]),
            8,
        );
        let event = DrcPaymentOutboxEvent::from_tx(&tx);

        assert_eq!(event.payment_id, tx.payment_id());
        assert_eq!(event.payment_version, DRC_PAYMENT_VERSION);
        assert_eq!(event.from, tx.from);
        assert_eq!(event.to, tx.to);
        assert_eq!(event.amount, tx.amount);
        assert_eq!(event.source_tag, tx.source_tag);
        assert_eq!(event.destination_tag, tx.destination_tag);
        assert_eq!(event.invoice_id, tx.invoice_id);
        let bytes = borsh::to_vec(&event).unwrap();
        assert_eq!(
            DrcPaymentOutboxEvent::try_from_slice(&bytes).unwrap(),
            event
        );
    }

    #[test]
    fn legacy_outbox_bytes_decode_without_a_source_tag() {
        let tx = legacy_payment();
        let event = DrcPaymentOutboxEvent::from_tx(&tx);
        let expected_bytes = borsh::to_vec(&LegacyDrcPaymentOutboxEvent {
            payment_id: event.payment_id,
            from: event.from,
            to: event.to,
            amount: event.amount,
            destination_tag: event.destination_tag,
            invoice_id: event.invoice_id,
        })
        .unwrap();
        assert_eq!(borsh::to_vec(&event).unwrap(), expected_bytes);
        assert_eq!(
            DrcPaymentOutboxEvent::try_from_slice(&expected_bytes).unwrap(),
            event
        );
    }

    #[test]
    fn exact_delivery_receipt_v1_encoding_is_locked() {
        let tx = DrcPaymentTx::unsigned_v2(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::from_base_units(4),
            5,
            Some(6),
            Hash([7u8; 32]),
            8,
        );
        let receipt = DrcPaymentReceipt::delivered_exact(&tx);
        let expected = borsh::to_vec(&DrcPaymentReceiptV1 {
            version: DRC_PAYMENT_RECEIPT_VERSION,
            payment_id: tx.payment_id(),
            payment_version: DRC_PAYMENT_VERSION,
            result: DrcPaymentResult::DeliveredExact,
            from: tx.from,
            to: tx.to,
            requested_amount: tx.amount,
            delivered_amount: tx.amount,
            fee_paid: tx.fee,
            source_tag: tx.source_tag,
            destination_tag: tx.destination_tag,
            invoice_id: tx.invoice_id,
        })
        .unwrap();

        assert_eq!(borsh::to_vec(&receipt).unwrap(), expected);
        assert_eq!(expected.len(), 146);
        assert_eq!(
            DrcPaymentReceipt::try_from_slice(&expected).unwrap(),
            receipt
        );
        receipt.validate_exact().unwrap();
        assert_eq!(receipt.result.as_str(), "delivered_exact");
    }

    #[test]
    fn partial_delivery_receipt_fails_closed() {
        let tx = DrcPaymentTx::unsigned_v2(
            Address([1u8; 20]),
            Address([2u8; 20]),
            Amount::from_base_units(3),
            Amount::ZERO,
            0,
            None,
            Hash::ZERO,
            0,
        );
        let mut malformed = DrcPaymentReceipt::delivered_exact(&tx);
        malformed.delivered_amount = Amount::from_base_units(2);
        assert_eq!(
            malformed.validate_exact(),
            Err(DrcPaymentReceiptError::AmountMismatch)
        );
    }
}
