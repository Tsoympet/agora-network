//! Entire-network light checks that sit on top of TLT Merkle inclusion.
//!
//! A passing check shows that a transaction id is in the supplied TLT Merkle root
//! and reports the Trident checkpoint state from PoW plus independent OVL and DRC
//! stake totals. It does not replace full-node RandomX verification or validator
//! attestation checks. Those stay on the existing PoW and checkpoint verifiers.
//!
//! For UTXO-only blocks, `header.tx_root` is this Merkle root. Multi-lane bodies
//! wrap it, so callers must not treat a mismatched header root as inclusion.

use agora_types::{verify_tlt_tx_merkle, CheckpointState, Hash, TltTxMerkleProof};

use crate::finality::evaluate_checkpoint_state;

/// Inputs a light client can check without replaying the UTXO set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TridentLightCheck {
    pub tx_merkle_root: Hash,
    pub inclusion: TltTxMerkleProof,
    /// When set, must equal `tx_merkle_root` (UTXO-only header binding).
    pub header_tx_root: Option<Hash>,
    pub pow_work_met: bool,
    pub ovl_signed_stake: u64,
    pub ovl_active_stake: u64,
    pub drc_signed_stake: u64,
    pub drc_active_stake: u64,
}

/// Why a light check failed before finality state is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TridentLightError {
    MerkleMismatch,
    HeaderRootMismatch,
}

/// Verify TLT inclusion, optional UTXO-only header binding, and dual-quorum state.
pub fn verify_trident_light_check(
    check: &TridentLightCheck,
) -> Result<CheckpointState, TridentLightError> {
    if !verify_tlt_tx_merkle(&check.tx_merkle_root, &check.inclusion) {
        return Err(TridentLightError::MerkleMismatch);
    }
    if let Some(header_root) = &check.header_tx_root {
        if header_root != &check.tx_merkle_root {
            return Err(TridentLightError::HeaderRootMismatch);
        }
    }
    Ok(evaluate_checkpoint_state(
        check.pow_work_met,
        check.ovl_signed_stake,
        check.ovl_active_stake,
        check.drc_signed_stake,
        check.drc_active_stake,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{prove_tlt_tx_merkle, tlt_tx_merkle_root};

    fn proof() -> (Hash, TltTxMerkleProof) {
        let leaves = [Hash([1u8; 32]), Hash([2u8; 32]), Hash([3u8; 32])];
        let root = tlt_tx_merkle_root(&leaves);
        let proof = prove_tlt_tx_merkle(&leaves, 1).unwrap();
        (root, proof)
    }

    #[test]
    fn finalized_only_when_pow_and_both_quorums_agree() {
        let (root, inclusion) = proof();
        let mut check = TridentLightCheck {
            tx_merkle_root: root,
            inclusion,
            header_tx_root: Some(root),
            pow_work_met: true,
            ovl_signed_stake: 2,
            ovl_active_stake: 3,
            drc_signed_stake: 2,
            drc_active_stake: 3,
        };
        assert_eq!(
            verify_trident_light_check(&check).unwrap(),
            CheckpointState::Finalized
        );
        check.drc_signed_stake = 1;
        assert_eq!(
            verify_trident_light_check(&check).unwrap(),
            CheckpointState::AwaitingDrcQuorum
        );
        check.drc_signed_stake = 2;
        check.ovl_active_stake = 0;
        assert_eq!(
            verify_trident_light_check(&check).unwrap(),
            CheckpointState::AwaitingOvlQuorum
        );
        check.ovl_active_stake = 3;
        check.pow_work_met = false;
        assert_eq!(
            verify_trident_light_check(&check).unwrap(),
            CheckpointState::Proposed
        );
    }

    #[test]
    fn rejects_bad_inclusion_and_unbound_header_root() {
        let (root, inclusion) = proof();
        let mut check = TridentLightCheck {
            tx_merkle_root: root,
            inclusion,
            header_tx_root: Some(Hash([9u8; 32])),
            pow_work_met: true,
            ovl_signed_stake: 3,
            ovl_active_stake: 3,
            drc_signed_stake: 3,
            drc_active_stake: 3,
        };
        assert_eq!(
            verify_trident_light_check(&check),
            Err(TridentLightError::HeaderRootMismatch)
        );
        check.header_tx_root = None;
        check.inclusion.tx_id = Hash([8u8; 32]);
        assert_eq!(
            verify_trident_light_check(&check),
            Err(TridentLightError::MerkleMismatch)
        );
    }
}
