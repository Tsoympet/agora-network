//! Dual-PoS + PoW checkpoint finality gadget.
//!
//! Pure consensus logic — no disk I/O. Callers supply active stake totals and
//! whether the PoW work threshold is met. There is **no** admin bypass that
//! silently drops an OVL or DRC quorum requirement.

use agora_types::{CheckpointBody, CheckpointState, FinalityCertificate};

use crate::quorum::has_two_thirds_quorum;
use crate::ConsensusError;

/// Policy knobs for the PoW leg of finality (integer only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalityPowPolicy {
    /// Minimum blue-score depth (or work units) required before PoS can finalize.
    pub min_pow_depth: u64,
}

impl Default for FinalityPowPolicy {
    fn default() -> Self {
        Self { min_pow_depth: 1 }
    }
}

/// Evaluate whether cumulative PoW depth meets policy.
pub fn pow_work_met(depth_or_work: u64, policy: &FinalityPowPolicy) -> bool {
    depth_or_work >= policy.min_pow_depth
}

/// Recompute checkpoint lifecycle from components.
///
/// Rules:
/// - Without PoW → stay `Proposed` (or `RevertedOrOrphaned` if caller marks orphan).
/// - With PoW but missing either PoS quorum → `PoWAccepted` / `AwaitingOvlQuorum` /
///   `AwaitingDrcQuorum` as appropriate — **never** `Finalized`.
/// - Empty active stake on either set → that quorum is **not** satisfied (no bypass).
pub fn evaluate_checkpoint_state(
    pow_met: bool,
    ovl_signed: u64,
    ovl_active: u64,
    drc_signed: u64,
    drc_active: u64,
) -> CheckpointState {
    if !pow_met {
        return CheckpointState::Proposed;
    }
    let ovl_ok = has_two_thirds_quorum(ovl_signed, ovl_active);
    let drc_ok = has_two_thirds_quorum(drc_signed, drc_active);
    match (ovl_ok, drc_ok) {
        (true, true) => CheckpointState::Finalized,
        (false, true) => CheckpointState::AwaitingOvlQuorum,
        (true, false) => CheckpointState::AwaitingDrcQuorum,
        (false, false) => CheckpointState::PoWAccepted,
    }
}

/// Refresh a certificate's state from its stake counters and PoW flag.
pub fn refresh_certificate(cert: &mut FinalityCertificate) {
    cert.state = evaluate_checkpoint_state(
        cert.pow_work_met,
        cert.ovl_signed_stake,
        cert.ovl_active_stake,
        cert.drc_signed_stake,
        cert.drc_active_stake,
    );
}

/// Mark PoW accepted when depth meets policy; never auto-finalizes without PoS.
pub fn note_pow_progress(
    cert: &mut FinalityCertificate,
    depth_or_work: u64,
    policy: &FinalityPowPolicy,
) {
    cert.pow_work_met = pow_work_met(depth_or_work, policy);
    refresh_certificate(cert);
}

/// Record signed stake for one set after validating the attestation belongs to `body`.
pub fn note_signed_stake(
    cert: &mut FinalityCertificate,
    body: &CheckpointBody,
    is_ovl: bool,
    signed_stake: u64,
    active_stake: u64,
) -> Result<(), ConsensusError> {
    if &cert.body != body {
        return Err(ConsensusError::InvalidBlock(
            "attestation body mismatch".into(),
        ));
    }
    if is_ovl {
        cert.ovl_signed_stake = signed_stake;
        cert.ovl_active_stake = active_stake;
    } else {
        cert.drc_signed_stake = signed_stake;
        cert.drc_active_stake = active_stake;
    }
    refresh_certificate(cert);
    Ok(())
}

/// Reject attempts to reorg past a finalized checkpoint.
pub fn assert_reorg_allowed(
    finalized_blue_score: Option<u64>,
    target_blue_score: u64,
) -> Result<(), ConsensusError> {
    if let Some(finalized) = finalized_blue_score {
        if target_blue_score <= finalized {
            return Err(ConsensusError::InvalidBlock(format!(
                "reorg beyond finality: target {target_blue_score} <= finalized {finalized}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_types::{CheckpointBody, Hash};

    use super::*;

    fn body() -> CheckpointBody {
        CheckpointBody {
            chain_id: "agora-trident-testnet-1".into(),
            genesis_hash: Hash([1u8; 32]),
            consensus_policy_hash: Hash([2u8; 32]),
            state_transition_version: "agora-trident-state-v1".into(),
            blue_score: 10,
            block_hash: Hash([3u8; 32]),
            state_root: Hash([4u8; 32]),
            validator_epoch: 1,
        }
    }

    #[test]
    fn pow_only_remains_unfinalized() {
        let s = evaluate_checkpoint_state(true, 0, 100, 0, 100);
        assert_eq!(s, CheckpointState::PoWAccepted);
        assert!(!s.is_finalized());
    }

    #[test]
    fn ovl_without_drc_unfinalized() {
        let s = evaluate_checkpoint_state(true, 70, 100, 0, 100);
        assert_eq!(s, CheckpointState::AwaitingDrcQuorum);
    }

    #[test]
    fn drc_without_ovl_unfinalized() {
        let s = evaluate_checkpoint_state(true, 0, 100, 70, 100);
        assert_eq!(s, CheckpointState::AwaitingOvlQuorum);
    }

    #[test]
    fn both_pos_without_pow_unfinalized() {
        let s = evaluate_checkpoint_state(false, 70, 100, 70, 100);
        assert_eq!(s, CheckpointState::Proposed);
    }

    #[test]
    fn empty_validator_set_no_bypass() {
        // Active=0 → quorum false even if signed looks full.
        let s = evaluate_checkpoint_state(true, 0, 0, 0, 0);
        assert_eq!(s, CheckpointState::PoWAccepted);
        let s = evaluate_checkpoint_state(true, 100, 0, 100, 0);
        assert_eq!(s, CheckpointState::PoWAccepted);
    }

    #[test]
    fn full_triple_finalizes() {
        let s = evaluate_checkpoint_state(true, 70, 100, 70, 100);
        assert_eq!(s, CheckpointState::Finalized);
    }

    #[test]
    fn certificate_refresh_and_reorg_guard() {
        let mut cert = FinalityCertificate::new(body());
        note_pow_progress(&mut cert, 1, &FinalityPowPolicy::default());
        assert_eq!(cert.state, CheckpointState::PoWAccepted);
        note_signed_stake(&mut cert, &body(), true, 70, 100).unwrap();
        assert_eq!(cert.state, CheckpointState::AwaitingDrcQuorum);
        note_signed_stake(&mut cert, &body(), false, 70, 100).unwrap();
        assert!(cert.state.is_finalized());
        assert!(assert_reorg_allowed(Some(10), 10).is_err());
        assert!(assert_reorg_allowed(Some(10), 11).is_ok());
    }
}
