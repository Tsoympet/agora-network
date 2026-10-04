//! Owner-controlled DRC weighted signer lists (secp256k1, contract-free).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{
    read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth, DRC_SIGNER_LIST_MAX_ENTRIES,
};
use crate::drc_sequence::DrcAccountSequenceSelector;
use crate::{Address, Amount, Hash};

pub const DRC_SIGNER_LIST_TX_VERSION: u32 = 1;
/// Ticket-aware signer-list operations bind an explicit sequence selector.
pub const DRC_SIGNER_LIST_TICKET_TX_VERSION: u32 = 2;
pub const DRC_SIGNER_LIST_STATE_VERSION: u32 = 1;
pub const DRC_SIGNER_LIST_TX_TYPE: &[u8] = b"drc_signer_list";
pub const DRC_SIGNER_LIST_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-signer-list-v1";
pub const DRC_SIGNER_LIST_V2_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-signer-list-v2";

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum DrcSignerListAction {
    /// Replace the installed list with canonical `entries` and `quorum`.
    Set = 0,
    /// Remove any installed signer list (master recovery remains).
    Delete = 1,
}

impl DrcSignerListAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Delete => "delete",
        }
    }
}

/// Canonical `(signer identity, weight)` entry; list order is by ascending signer address.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcSignerListEntry {
    pub signer: Address,
    pub weight: u16,
}

/// Owner-signed operation that installs, replaces, or deletes one signer list.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcSignerListTx {
    pub version: u32,
    pub owner: Address,
    pub action: DrcSignerListAction,
    pub quorum: u32,
    pub entries: Vec<DrcSignerListEntry>,
    pub fee: Amount,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sequence: Option<DrcAccountSequenceSelector>,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcSignerListTx {
    pub fn validate_structure(&self) -> Result<(), DrcSignerListError> {
        if self.version != DRC_SIGNER_LIST_TX_VERSION
            && self.version != DRC_SIGNER_LIST_TICKET_TX_VERSION
        {
            return Err(DrcSignerListError::UnsupportedVersion(self.version));
        }
        if self.version == DRC_SIGNER_LIST_TX_VERSION
            && self
                .account_sequence
                .is_some_and(|s| s.kind == crate::drc_sequence::DrcAccountSequence::Ticket)
        {
            return Err(DrcSignerListError::TicketSelectorOnLegacyVersion);
        }
        if self.owner == Address::ZERO {
            return Err(DrcSignerListError::ZeroOwner);
        }
        match self.action {
            DrcSignerListAction::Set => {
                validate_signer_list_payload(&self.owner, &self.entries, self.quorum)
            }
            DrcSignerListAction::Delete => {
                if self.quorum != 0 || !self.entries.is_empty() {
                    return Err(DrcSignerListError::DeleteMustBeEmpty);
                }
                Ok(())
            }
        }
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        if self.version >= DRC_SIGNER_LIST_TICKET_TX_VERSION {
            let sequence = self
                .account_sequence
                .expect("signer-list v2 requires account_sequence");
            return borsh::to_vec(&(
                DRC_SIGNER_LIST_V2_SIGNING_DOMAIN,
                chain_id,
                genesis.as_bytes(),
                DRC_SIGNER_LIST_TX_TYPE,
                self.version,
                self.owner,
                self.action,
                self.quorum,
                &self.entries,
                self.fee,
                sequence,
            ))
            .expect("borsh serialize DRC signer-list v2 body");
        }
        borsh::to_vec(&(
            DRC_SIGNER_LIST_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_SIGNER_LIST_TX_TYPE,
            self.version,
            self.owner,
            self.action,
            self.quorum,
            &self.entries,
            self.nonce,
            self.fee,
        ))
        .expect("borsh serialize DRC signer-list body")
    }

    pub fn signer_list_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned_set(
        owner: Address,
        entries: Vec<DrcSignerListEntry>,
        quorum: u32,
        fee: Amount,
        nonce: u64,
    ) -> Self {
        Self {
            version: DRC_SIGNER_LIST_TX_VERSION,
            owner,
            action: DrcSignerListAction::Set,
            quorum,
            entries,
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: None,
        }
    }

    pub fn unsigned_delete(owner: Address, fee: Amount, nonce: u64) -> Self {
        Self {
            version: DRC_SIGNER_LIST_TX_VERSION,
            owner,
            action: DrcSignerListAction::Delete,
            quorum: 0,
            entries: Vec::new(),
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
            account_sequence: None,
        }
    }
}

impl BorshSerialize for DrcSignerListTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.action, writer)?;
        BorshSerialize::serialize(&self.quorum, writer)?;
        BorshSerialize::serialize(&self.entries, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        if self.version >= DRC_SIGNER_LIST_TICKET_TX_VERSION {
            BorshSerialize::serialize(
                &self
                    .account_sequence
                    .expect("signer-list v2 missing account_sequence"),
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

impl BorshDeserialize for DrcSignerListTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let version = u32::deserialize_reader(reader)?;
        let owner = Address::deserialize_reader(reader)?;
        let action = DrcSignerListAction::deserialize_reader(reader)?;
        let quorum = u32::deserialize_reader(reader)?;
        let entries = Vec::<DrcSignerListEntry>::deserialize_reader(reader)?;
        let fee = Amount::deserialize_reader(reader)?;
        let (nonce, account_sequence) = if version >= DRC_SIGNER_LIST_TICKET_TX_VERSION {
            (
                0,
                Some(DrcAccountSequenceSelector::deserialize_reader(reader)?),
            )
        } else {
            (u64::deserialize_reader(reader)?, None)
        };
        Ok(Self {
            version,
            owner,
            action,
            quorum,
            entries,
            fee,
            nonce,
            account_sequence,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

/// Canonical installed signer list for one owner account.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcAccountSignerList {
    pub version: u32,
    pub owner: Address,
    pub quorum: u32,
    pub entries: Vec<DrcSignerListEntry>,
}

impl DrcAccountSignerList {
    pub fn validate(&self) -> Result<(), DrcSignerListError> {
        if self.version != DRC_SIGNER_LIST_STATE_VERSION {
            return Err(DrcSignerListError::UnsupportedStateVersion(self.version));
        }
        validate_signer_list_payload(&self.owner, &self.entries, self.quorum)?;
        if canonical_sorted_entries(&self.entries) != self.entries {
            return Err(DrcSignerListError::EntriesNotCanonicalOrder);
        }
        Ok(())
    }
}

pub fn validate_signer_list_payload(
    owner: &Address,
    entries: &[DrcSignerListEntry],
    quorum: u32,
) -> Result<(), DrcSignerListError> {
    if entries.is_empty() {
        return Err(DrcSignerListError::EmptyList);
    }
    if entries.len() > DRC_SIGNER_LIST_MAX_ENTRIES {
        return Err(DrcSignerListError::TooManyEntries(entries.len()));
    }
    if quorum == 0 {
        return Err(DrcSignerListError::ZeroQuorum);
    }
    let mut total: u64 = 0;
    let mut last: Option<Address> = None;
    for entry in entries {
        if entry.signer == Address::ZERO {
            return Err(DrcSignerListError::ZeroSigner);
        }
        if entry.signer == *owner {
            return Err(DrcSignerListError::OwnerAsSigner);
        }
        if entry.weight == 0 {
            return Err(DrcSignerListError::ZeroWeight);
        }
        if let Some(prev) = last {
            if entry.signer.0 <= prev.0 {
                return Err(DrcSignerListError::UnsortedOrDuplicateSigner);
            }
        }
        last = Some(entry.signer);
        total = total
            .checked_add(u64::from(entry.weight))
            .ok_or(DrcSignerListError::WeightOverflow)?;
    }
    if u64::from(quorum) > total {
        return Err(DrcSignerListError::ImpossibleQuorum);
    }
    Ok(())
}

pub fn canonical_sorted_entries(entries: &[DrcSignerListEntry]) -> Vec<DrcSignerListEntry> {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| entry.signer.0);
    sorted
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcSignerListError {
    #[error("unsupported DRC signer-list transaction version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC signer-list state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("DRC signer-list owner must be non-zero")]
    ZeroOwner,
    #[error("signer-list delete must leave quorum/entries empty")]
    DeleteMustBeEmpty,
    #[error("signer list must not be empty")]
    EmptyList,
    #[error("too many signer-list entries: {0}")]
    TooManyEntries(usize),
    #[error("signer-list quorum must be non-zero")]
    ZeroQuorum,
    #[error("signer-list signer must be non-zero")]
    ZeroSigner,
    #[error("owner cannot appear on its own signer list")]
    OwnerAsSigner,
    #[error("signer weight must be non-zero")]
    ZeroWeight,
    #[error("signer-list entries must be strictly sorted by signer without duplicates")]
    UnsortedOrDuplicateSigner,
    #[error("signer-list weight sum overflow")]
    WeightOverflow,
    #[error("signer-list quorum exceeds total weight")]
    ImpossibleQuorum,
    #[error("persisted signer-list entries are not in canonical order")]
    EntriesNotCanonicalOrder,
    #[error("ticket sequence selector requires a ticket-capable operation version")]
    TicketSelectorOnLegacyVersion,
}
