//! Signed staking operations for OVL / DRC validator sets (Trident L1).
//!
//! Distinct from [`crate::AccountTransfer`]: stake ops lock/unlock stake and
//! register consensus keys — they are not peer liquid transfers.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Hash, NativeAssetId};

/// Domain separator for stake transaction signatures.
pub const STAKE_TX_SIGNING_DOMAIN: &[u8] = b"agora-trident-stake-tx-v1";
/// DRC ticket-aware stake operations bind an explicit sequence selector.
pub const STAKE_TX_SIGNING_DOMAIN_V2: &[u8] = b"agora-trident-stake-tx-v2";
pub const STAKE_TX_VERSION: u32 = 1;
pub const STAKE_TX_TICKET_VERSION: u32 = 2;

/// Kind of staking mutation.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub enum StakeOpKind {
    Bond,
    Delegate,
    UnbondSelf,
    Withdraw,
}

impl StakeOpKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bond => "Bond",
            Self::Delegate => "Delegate",
            Self::UnbondSelf => "UnbondSelf",
            Self::Withdraw => "Withdraw",
        }
    }
}

/// Network-bound signed stake transaction.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
pub struct SignedStakeTx {
    pub version: u32,
    pub asset: NativeAssetId,
    pub kind: StakeOpKind,
    /// Signer / liquid debit account (must match pubkey).
    pub actor: Address,
    /// Bond: operator (= actor). Delegate: target validator. Unbond/Withdraw: operator/owner.
    pub validator: Address,
    /// Bond / Delegate amount; ignored for UnbondSelf / Withdraw (full self-bond / matured).
    pub amount: u64,
    /// Bond only: 33-byte compressed consensus pubkey.
    pub consensus_pubkey: Vec<u8>,
    /// Bond only: withdrawal address (receives rewards / unbond credits).
    pub withdrawal: Address,
    /// Bond only: commission in basis points.
    pub commission_bps: u16,
    pub metadata_hash: Hash,
    /// Actor account nonce (must match current on-chain nonce).
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl SignedStakeTx {
    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= STAKE_TX_TICKET_VERSION {
            let sequence = self
                .account_sequence
                .expect("stake v2 requires account_sequence");
            let body = (
                STAKE_TX_SIGNING_DOMAIN_V2,
                chain_id,
                genesis.as_bytes(),
                self.version,
                self.asset,
                self.kind,
                self.actor,
                self.validator,
                self.amount,
                &self.consensus_pubkey,
                self.withdrawal,
                self.commission_bps,
                self.metadata_hash,
                sequence,
            );
            return borsh::to_vec(&body).expect("borsh serialize stake tx v2 body");
        }
        let body = (
            STAKE_TX_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            self.version,
            self.asset,
            self.kind,
            self.actor,
            self.validator,
            self.amount,
            &self.consensus_pubkey,
            self.withdrawal,
            self.commission_bps,
            self.metadata_hash,
            self.nonce,
        );
        borsh::to_vec(&body).expect("borsh serialize stake tx body")
    }

    pub fn stake_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned_bond(
        asset: NativeAssetId,
        actor: Address,
        amount: u64,
        consensus_pubkey: Vec<u8>,
        withdrawal: Address,
        commission_bps: u16,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::Bond,
            actor,
            validator: actor,
            amount,
            consensus_pubkey,
            withdrawal,
            commission_bps,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: None,
        }
    }

    pub fn unsigned_bond_v2(
        actor: Address,
        amount: u64,
        consensus_pubkey: Vec<u8>,
        withdrawal: Address,
        commission_bps: u16,
        account_sequence: DrcAccountSequenceSelector,
    ) -> Self {
        Self {
            version: STAKE_TX_TICKET_VERSION,
            asset: NativeAssetId::DRC,
            kind: StakeOpKind::Bond,
            actor,
            validator: actor,
            amount,
            consensus_pubkey,
            withdrawal,
            commission_bps,
            metadata_hash: Hash::ZERO,
            nonce: 0,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: Some(account_sequence),
        }
    }

    pub fn unsigned_delegate(
        asset: NativeAssetId,
        actor: Address,
        validator: Address,
        amount: u64,
        nonce: u64,
    ) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::Delegate,
            actor,
            validator,
            amount,
            consensus_pubkey: Vec::new(),
            withdrawal: Address::ZERO,
            commission_bps: 0,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: None,
        }
    }

    pub fn unsigned_unbond_self(asset: NativeAssetId, actor: Address, nonce: u64) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::UnbondSelf,
            actor,
            validator: actor,
            amount: 0,
            consensus_pubkey: Vec::new(),
            withdrawal: Address::ZERO,
            commission_bps: 0,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: None,
        }
    }

    pub fn unsigned_withdraw(asset: NativeAssetId, actor: Address, nonce: u64) -> Self {
        Self {
            version: 1,
            asset,
            kind: StakeOpKind::Withdraw,
            actor,
            validator: actor,
            amount: 0,
            consensus_pubkey: Vec::new(),
            withdrawal: Address::ZERO,
            commission_bps: 0,
            metadata_hash: Hash::ZERO,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: None,
        }
    }
}

impl BorshSerialize for SignedStakeTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.asset, writer)?;
        BorshSerialize::serialize(&self.kind, writer)?;
        BorshSerialize::serialize(&self.actor, writer)?;
        BorshSerialize::serialize(&self.validator, writer)?;
        BorshSerialize::serialize(&self.amount, writer)?;
        BorshSerialize::serialize(&self.consensus_pubkey, writer)?;
        BorshSerialize::serialize(&self.withdrawal, writer)?;
        BorshSerialize::serialize(&self.commission_bps, writer)?;
        BorshSerialize::serialize(&self.metadata_hash, writer)?;
        if self.version >= STAKE_TX_TICKET_VERSION {
            BorshSerialize::serialize(
                &self
                    .account_sequence
                    .expect("stake v2 missing account_sequence"),
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

impl BorshDeserialize for SignedStakeTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let version = u32::deserialize_reader(reader)?;
        let asset = NativeAssetId::deserialize_reader(reader)?;
        let kind = StakeOpKind::deserialize_reader(reader)?;
        let actor = Address::deserialize_reader(reader)?;
        let validator = Address::deserialize_reader(reader)?;
        let amount = u64::deserialize_reader(reader)?;
        let consensus_pubkey = Vec::<u8>::deserialize_reader(reader)?;
        let withdrawal = Address::deserialize_reader(reader)?;
        let commission_bps = u16::deserialize_reader(reader)?;
        let metadata_hash = Hash::deserialize_reader(reader)?;
        let (nonce, account_sequence) = if version >= STAKE_TX_TICKET_VERSION {
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
            kind,
            actor,
            validator,
            amount,
            consensus_pubkey,
            withdrawal,
            commission_bps,
            metadata_hash,
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
    fn stake_tx_id_stable() {
        let tx = SignedStakeTx::unsigned_bond(
            NativeAssetId::OVL,
            Address::ZERO,
            100,
            vec![2; 33],
            Address::ZERO,
            0,
            0,
        );
        assert_eq!(tx.stake_tx_id(), tx.stake_tx_id());
        assert_eq!(tx.kind.as_str(), "Bond");
    }
}
