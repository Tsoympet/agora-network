//! Signed DRC payment envelopes and outbox events for Trident L1.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Address, Amount, Hash};

/// Domain separator for network-bound DRC payment signatures.
pub const DRC_PAYMENT_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-payment-v1";

/// Signed account-based DRC payment.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
pub struct DrcPaymentTx {
    pub version: u32,
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    pub fee: Amount,
    /// `0` indicates that the destination does not require a tag.
    pub destination_tag: u32,
    /// `Hash::ZERO` indicates that the payment is not associated with an invoice.
    pub invoice_id: Hash,
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl DrcPaymentTx {
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        let body = (
            DRC_PAYMENT_SIGNING_DOMAIN,
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
        );
        borsh::to_vec(&body).expect("borsh serialize DRC payment body")
    }

    /// Hashes the complete signed envelope, including authorization material.
    pub fn payment_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

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
            version: 1,
            from,
            to,
            amount,
            fee,
            destination_tag,
            invoice_id,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }
}

/// Durable notification emitted after accepting a DRC payment.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcPaymentOutboxEvent {
    pub payment_id: Hash,
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    pub destination_tag: u32,
    pub invoice_id: Hash,
}

impl DrcPaymentOutboxEvent {
    pub fn from_tx(tx: &DrcPaymentTx) -> Self {
        Self {
            payment_id: tx.payment_id(),
            from: tx.from,
            to: tx.to,
            amount: tx.amount,
            destination_tag: tx.destination_tag,
            invoice_id: tx.invoice_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment() -> DrcPaymentTx {
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
    fn payment_id_is_stable_and_covers_auth() {
        let mut tx = payment();
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
    fn outbox_event_copies_payment_routing_fields() {
        let tx = payment();
        let event = DrcPaymentOutboxEvent::from_tx(&tx);

        assert_eq!(event.payment_id, tx.payment_id());
        assert_eq!(event.from, tx.from);
        assert_eq!(event.to, tx.to);
        assert_eq!(event.amount, tx.amount);
        assert_eq!(event.destination_tag, tx.destination_tag);
        assert_eq!(event.invoice_id, tx.invoice_id);
    }
}
