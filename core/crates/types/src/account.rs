//! Native account-transfer envelope for OVL/DRC (Trident L1).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::{Address, Amount, Hash, NativeAssetId};

/// Domain separator for native account transfers (includes fee field).
pub const ACCOUNT_TX_SIGNING_DOMAIN: &[u8] = b"agora-trident-account-tx-v2";

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
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl AccountTransfer {
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
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
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for AccountTransfer {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            asset: NativeAssetId::deserialize_reader(reader)?,
            from: Address::deserialize_reader(reader)?,
            to: Address::deserialize_reader(reader)?,
            amount: Amount::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            nonce: u64::deserialize_reader(reader)?,
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
