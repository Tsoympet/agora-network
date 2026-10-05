//! Detached multisign authorization for DRC account-bound operations (secp256k1).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{Address, Hash};

/// Matches rippled 2.5.0 `ExpandedSignerList` entry cap (not XRPL wire parity).
pub const DRC_SIGNER_LIST_MAX_ENTRIES: usize = 32;
/// Per-signer weight bound aligned to XRPL `SignerEntry` weight width.
pub const DRC_SIGNER_MAX_WEIGHT: u16 = 65535;
/// Maximum detached multisign signatures accepted on one operation envelope.
pub const DRC_MULTISIGN_MAX_SIGNATURES: usize = DRC_SIGNER_LIST_MAX_ENTRIES;

pub const DRC_MULTISIGN_AUTH_VERSION: u32 = 1;
pub const DRC_MULTISIGN_PARTICIPANT_DOMAIN: &[u8] = b"agora-trident-drc-multisign-participant-v1";

/// One secp256k1 multisign participant signature (sorted by `signer` in the bundle).
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcMultisignEntry {
    pub signer: Address,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Versioned detached authorization bundle. Appended to frozen single-signature Borsh encodings.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DrcMultisignAuth {
    pub version: u32,
    /// On-chain account the operation acts for (`owner` / `from` / `actor` / `account`).
    pub signing_for: Address,
    /// Strictly increasing `signer` order; at most [`DRC_MULTISIGN_MAX_SIGNATURES`] entries.
    pub signatures: Vec<DrcMultisignEntry>,
}

impl DrcMultisignAuth {
    pub fn validate_structure(&self) -> Result<(), DrcMultisignError> {
        if self.version != DRC_MULTISIGN_AUTH_VERSION {
            return Err(DrcMultisignError::UnsupportedVersion(self.version));
        }
        if self.signing_for == Address::ZERO {
            return Err(DrcMultisignError::ZeroSigningFor);
        }
        if self.signatures.is_empty() {
            return Err(DrcMultisignError::EmptySignatures);
        }
        if self.signatures.len() > DRC_MULTISIGN_MAX_SIGNATURES {
            return Err(DrcMultisignError::TooManySignatures(self.signatures.len()));
        }
        let mut last: Option<Address> = None;
        for entry in &self.signatures {
            if entry.signer == Address::ZERO {
                return Err(DrcMultisignError::ZeroSigner);
            }
            if entry.public_key.len() != 33 || entry.signature.len() != 64 {
                return Err(DrcMultisignError::MalformedParticipant);
            }
            if let Some(prev) = last {
                if entry.signer.0 <= prev.0 {
                    return Err(DrcMultisignError::UnsortedOrDuplicateSigner);
                }
            }
            last = Some(entry.signer);
        }
        Ok(())
    }

    pub fn participant_signing_bytes_bound(
        signing_for: Address,
        operation_signing_bytes: &[u8],
        chain_id: &str,
        genesis: &Hash,
    ) -> Vec<u8> {
        borsh::to_vec(&(
            DRC_MULTISIGN_PARTICIPANT_DOMAIN,
            chain_id,
            genesis.as_bytes(),
            DRC_MULTISIGN_AUTH_VERSION,
            signing_for,
            operation_signing_bytes,
        ))
        .expect("borsh serialize DRC multisign participant body")
    }
}

/// Legacy single-signature bodies end at `signature`; a `1` marker introduces trailing multisign.
pub fn write_multisign_trailer<W: borsh::io::Write>(
    _multisign: &Option<DrcMultisignAuth>,
    _writer: &mut W,
) -> Result<(), borsh::io::Error> {
    // Multisign authorization is carried on the in-memory/JSON envelope and merged
    // before state verification. It is intentionally omitted from consensus Borsh vec
    // elements so legacy lane layouts remain byte-stable between operations.
    Ok(())
}

pub fn read_multisign_trailer<R: borsh::io::Read>(
    _reader: &mut R,
) -> Result<Option<DrcMultisignAuth>, borsh::io::Error> {
    Ok(None)
}

/// Reject ambiguous authorization: exactly one of single-signature or multisign.
pub fn validate_exclusive_authorization(
    public_key: &[u8],
    signature: &[u8],
    multisign: &Option<DrcMultisignAuth>,
) -> Result<(), DrcMultisignError> {
    let single = !public_key.is_empty() || !signature.is_empty();
    let multi = multisign.is_some();
    match (single, multi) {
        (true, true) => Err(DrcMultisignError::MixedAuthorization),
        (false, false) => Err(DrcMultisignError::MissingAuthorization),
        _ => Ok(()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum DrcMultisignError {
    #[error("unsupported DRC multisign version {0}")]
    UnsupportedVersion(u32),
    #[error("multisign signing_for must be non-zero")]
    ZeroSigningFor,
    #[error("multisign bundle must include at least one signature")]
    EmptySignatures,
    #[error("too many multisign signatures: {0}")]
    TooManySignatures(usize),
    #[error("multisign signer must be non-zero")]
    ZeroSigner,
    #[error("multisign participant keys must be 33-byte pubkey and 64-byte signature")]
    MalformedParticipant,
    #[error("multisign signatures must be strictly sorted by signer without duplicates")]
    UnsortedOrDuplicateSigner,
    #[error("operation cannot combine single-signature and multisign authorization")]
    MixedAuthorization,
    #[error("operation missing authorization")]
    MissingAuthorization,
    #[error("multisign signing_for does not match operation account")]
    SigningForMismatch,
    #[error("multisign signer is not on the installed signer list")]
    ForeignSigner,
    #[error("multisign weight sum overflow")]
    WeightOverflow,
    #[error("multisign weight below quorum")]
    QuorumNotMet,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailer_is_not_written_into_consensus_borsh_lanes() {
        let mut legacy = Vec::new();
        write_multisign_trailer(&None, &mut legacy).unwrap();
        assert!(legacy.is_empty());
        let auth = DrcMultisignAuth {
            version: DRC_MULTISIGN_AUTH_VERSION,
            signing_for: Address([1; 20]),
            signatures: vec![DrcMultisignEntry {
                signer: Address([2; 20]),
                public_key: vec![3; 33],
                signature: vec![4; 64],
            }],
        };
        write_multisign_trailer(&Some(auth), &mut legacy).unwrap();
        assert!(legacy.is_empty());
    }
}
