//! DRC one-use ticket creation (rippled Tickets subset, contract-free).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::drc_multisign::{read_multisign_trailer, write_multisign_trailer, DrcMultisignAuth};
use crate::{Address, Amount, Hash};

pub const DRC_TICKET_CREATE_TX_VERSION: u32 = 1;
pub const DRC_TICKET_STATE_VERSION: u32 = 1;
pub const DRC_TICKET_CREATE_TX_TYPE: &[u8] = b"drc_ticket_create";
pub const DRC_TICKET_CREATE_SIGNING_DOMAIN: &[u8] = b"agora-trident-drc-ticket-create-v1";

/// Owner-signed creation of exactly one outstanding ticket (nonce-only; never consumes a ticket).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcTicketCreateTx {
    pub version: u32,
    pub owner: Address,
    pub fee: Amount,
    /// Ordinary shared DRC nonce at creation time (must equal current account nonce).
    pub nonce: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisign: Option<DrcMultisignAuth>,
}

impl DrcTicketCreateTx {
    pub fn validate_structure(&self) -> Result<(), DrcTicketCreateError> {
        if self.version != DRC_TICKET_CREATE_TX_VERSION {
            return Err(DrcTicketCreateError::UnsupportedVersion(self.version));
        }
        if self.owner == Address::ZERO {
            return Err(DrcTicketCreateError::ZeroOwner);
        }
        Ok(())
    }

    pub fn signing_bytes_bound(&self, chain_id: &str, genesis: &Hash) -> Vec<u8> {
        borsh::to_vec(&(
            DRC_TICKET_CREATE_SIGNING_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_TICKET_CREATE_TX_TYPE,
            self.version,
            self.owner,
            self.nonce,
            self.fee,
        ))
        .expect("borsh serialize DRC ticket-create body")
    }

    pub fn ticket_create_tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned(owner: Address, fee: Amount, nonce: u64) -> Self {
        Self {
            version: DRC_TICKET_CREATE_TX_VERSION,
            owner,
            fee,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
            multisign: None,
        }
    }
}

impl BorshSerialize for DrcTicketCreateTx {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.owner, writer)?;
        BorshSerialize::serialize(&self.fee, writer)?;
        BorshSerialize::serialize(&self.nonce, writer)?;
        BorshSerialize::serialize(&self.public_key, writer)?;
        BorshSerialize::serialize(&self.signature, writer)?;
        write_multisign_trailer(&self.multisign, writer)
    }
}

impl BorshDeserialize for DrcTicketCreateTx {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            owner: Address::deserialize_reader(reader)?,
            fee: Amount::deserialize_reader(reader)?,
            nonce: u64::deserialize_reader(reader)?,
            public_key: Vec::<u8>::deserialize_reader(reader)?,
            signature: Vec::<u8>::deserialize_reader(reader)?,
            multisign: read_multisign_trailer(reader)?,
        })
    }
}

/// Canonical persisted ticket set for one owner (sorted sequences, capped).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct DrcAccountTickets {
    pub version: u32,
    pub owner: Address,
    /// Sorted strictly increasing ticket sequences.
    pub sequences: Vec<u64>,
}

impl DrcAccountTickets {
    pub fn validate(&self) -> Result<(), DrcTicketCreateError> {
        if self.version != DRC_TICKET_STATE_VERSION {
            return Err(DrcTicketCreateError::UnsupportedStateVersion(self.version));
        }
        if self.owner == Address::ZERO {
            return Err(DrcTicketCreateError::ZeroOwner);
        }
        if self.sequences.len() > crate::drc_sequence::DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT {
            return Err(DrcTicketCreateError::TicketCapExceeded);
        }
        for pair in self.sequences.windows(2) {
            if pair[0] >= pair[1] {
                return Err(DrcTicketCreateError::UnsortedTickets);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcTicketCreateError {
    #[error("unsupported DRC ticket-create version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DRC ticket state version {0}")]
    UnsupportedStateVersion(u32),
    #[error("zero DRC ticket owner")]
    ZeroOwner,
    #[error("DRC outstanding ticket cap exceeded")]
    TicketCapExceeded,
    #[error("unsorted DRC ticket sequences in state")]
    UnsortedTickets,
}
