//! Native account-transfer envelope for OVL/DRC (Trident L1).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash, NativeAssetId};

/// Domain separator for native account transfers (includes fee field).
pub const ACCOUNT_TX_SIGNING_DOMAIN: &[u8] = b"agora-trident-account-tx-v2";
/// DRC ticket-aware account transfers bind an explicit sequence selector.
pub const ACCOUNT_TX_SIGNING_DOMAIN_V3: &[u8] = b"agora-trident-account-tx-v3";
pub const ACCOUNT_TRANSFER_LEGACY_VERSION: u32 = 1;
pub const ACCOUNT_TRANSFER_VERSION: u32 = 2;
pub const ACCOUNT_TRANSFER_DRC_TICKET_VERSION: u32 = 3;

/// Signed account-to-account transfer for OVL or DRC.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
pub struct AccountTransfer {
    pub version: u32,
    pub asset: NativeAssetId,
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    /// Explicit same-asset fee (credited to staking reward pool when Accepted).
    pub fee: Amount,
    /// Sender account nonce (must match current on-chain nonce).
    pub nonce: u64,
    /// Explicit nonce/ticket selector for DRC v3+ (required when `version >= 3`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl AccountTransfer {
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= ACCOUNT_TRANSFER_DRC_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("v3 transfer requires account_sequence");
            let body = (
                ACCOUNT_TX_SIGNING_DOMAIN_V3,
                chain_id,
                genesis.as_bytes(),
                self.version,
                self.asset,
                self.from,
                self.to,
                self.amount,
                self.fee,
                sequence,
            );
            return borsh::to_vec(&body).expect("borsh serialize account transfer v3 body");
        }
        let body = (
            ACCOUNT_TX_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.asset,
            self.from,
            self.to,
            self.amount,
            self.fee,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize account transfer body")
    }

    pub fn effective_drc_sequence(&self) -> Result<DrcAccountSequenceSelector, &'static str> {
        if self.asset != NativeAssetId::DRC {
            return Err("not a DRC transfer");
        }
        if self.version >= ACCOUNT_TRANSFER_DRC_TICKET_VERSION {
            self.account_sequence.ok_or("missing account_sequence")
        } else {
            Ok(DrcAccountSequenceSelector::nonce(self.nonce))
        }
    }

    pub fn unsigned_with_fee_v3(
        asset: NativeAssetId,
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
        account_sequence: DrcAccountSequenceSelector,
    ) -> Self {
        Self {
            version: ACCOUNT_TRANSFER_DRC_TICKET_VERSION,
            asset,
            from,
            to,
            amount,
            fee,
            nonce: 0,
            account_sequence: Some(account_sequence),
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        }
    }

    pub fn transfer_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned(
        asset: NativeAssetId,
        from: Address,
        to: Address,
        amount: Amount,
        nonce: u64,
    ) -> Self {
        Self::unsigned_with_fee(asset, from, to, amount, Amount::ZERO, nonce)
    }

    pub fn unsigned_with_fee(
        asset: NativeAssetId,
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self {
            version: 2,
            asset,
            from,
            to,
            amount,
            fee,
            nonce,
            account_sequence: None,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        }
    }
}

impl BorshSerialize for AccountTransfer {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.asset, writer)?;
        BorshSerialize::serialize(&self.from, writer)?;
        BorshSerialize::serialize(&self.to, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        if self.version >= ACCOUNT_TRANSFER_DRC_TICKET_VERSION {
            BorshSerialize::serialize(
                &self
                    .account_sequence
                    .expect("v3 transfer missing account_sequence"),
                writer,
            )?;
        } else {
            BorshSerialize::serialize(&self.nonce, writer)?;
        }
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for AccountTransfer {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let version = u32::deserialize_reader(reader)?;
        let asset = NativeAssetId::deserialize_reader(reader)?;
        let from = Address::deserialize_reader(reader)?;
        let to = Address::deserialize_reader(reader)?;
        let amount = Amount::deserialize_reader(reader)?;
        let fee = Amount::deserialize_reader(reader)?;
        let (nonce, account_sequence) = if version >= ACCOUNT_TRANSFER_DRC_TICKET_VERSION {
            (
                0,
                Some(DrcAccountSequenceSelector::deserialize_reader(reader)?),
            )
        } else {
            (u64::deserialize_reader(reader)?, None)
        };
        Ok(Self {
            version,
            asset,
            from,
            to,
            amount,
            fee,
            nonce,
            account_sequence,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_transfer_id_stable() {
        let tx = AccountTransfer::unsigned(
            NativeAssetId::OVL,
            Address::ZERO,
            Address([1u8; 20]),
            Amount::from_base_units(9),
            0,
        );
        assert_eq!(tx.transfer_id(), tx.transfer_id());
        assert!(!tx.asset.is_mineable());
        assert_eq!(tx.fee.as_base_units(), 0);
    }
}
