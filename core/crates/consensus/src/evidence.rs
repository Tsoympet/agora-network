//! Equivocation / misbehavior evidence for Trident validators.

use agora_types::{Address, CheckpointAttestation, Hash, NativeAssetId};
use borsh::{BorshDeserialize, BorshSerialize};

/// Why a validator is penalized.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum EvidenceKind {
    /// Same validator signed two conflicting checkpoints in one epoch.
    DoubleCheckpointSignature,
    /// Conflicting attestation material (alias of double for clarity in APIs).
    ConflictingCheckpointSignature,
    /// Objectively false state root attestation (when provable).
    InvalidStateAttestation,
    /// Extended downtime (jail only by default).
    ExtendedDowntime,
    /// Key compromise report (process / optional jail).
    KeyCompromiseReport,
}

/// Slash / jail policy defaults (basis points of bonded stake).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashPolicy {
    /// Double / conflicting checkpoint — default 5% = 500 bps.
    pub equivocation_slash_bps: u16,
    /// Invalid-state attestation — default 1% = 100 bps.
    pub invalid_state_slash_bps: u16,
    /// Downtime slash — default 0.
    pub downtime_slash_bps: u16,
    pub jail_epochs_on_invalid_state: u64,
}

impl Default for SlashPolicy {
    fn default() -> Self {
        Self {
            equivocation_slash_bps: 500,
            invalid_state_slash_bps: 100,
            downtime_slash_bps: 0,
            jail_epochs_on_invalid_state: 1,
        }
    }
}

impl SlashPolicy {
    pub fn slash_amount(self, bonded: u64, bps: u16) -> u64 {
        let bps = u128::from(bps);
        let bonded = u128::from(bonded);
        ((bonded * bps) / 10_000) as u64
    }

    pub fn penalty_for(self, kind: &EvidenceKind, bonded: u64) -> (u64, bool, u64) {
        // (slash_amount, tombstone, jail_epochs)
        match kind {
            EvidenceKind::DoubleCheckpointSignature
            | EvidenceKind::ConflictingCheckpointSignature => (
                self.slash_amount(bonded, self.equivocation_slash_bps),
                true,
                0,
            ),
            EvidenceKind::InvalidStateAttestation => (
                self.slash_amount(bonded, self.invalid_state_slash_bps),
                false,
                self.jail_epochs_on_invalid_state,
            ),
            EvidenceKind::ExtendedDowntime => {
                (self.slash_amount(bonded, self.downtime_slash_bps), false, 1)
            }
            EvidenceKind::KeyCompromiseReport => (0, false, 1),
        }
    }
}

/// Evidence object retained for governance / slashing.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorEvidence {
    pub kind: EvidenceKind,
    pub set: NativeAssetId,
    pub validator: Address,
    pub epoch: u64,
    pub attestation_a: Option<CheckpointAttestation>,
    pub attestation_b: Option<CheckpointAttestation>,
    pub note_hash: Hash,
}

/// Detect double-sign: same validator, same epoch, conflicting checkpoint ids.
pub fn detect_double_checkpoint(
    a: &CheckpointAttestation,
    b: &CheckpointAttestation,
) -> Option<ValidatorEvidence> {
    if a.validator != b.validator || a.set != b.set {
        return None;
    }
    if a.body.validator_epoch != b.body.validator_epoch {
        return None;
    }
    if a.body.checkpoint_id() == b.body.checkpoint_id() {
        return None;
    }
    // Conflicting checkpoints in the same epoch.
    Some(ValidatorEvidence {
        kind: EvidenceKind::DoubleCheckpointSignature,
        set: a.set,
        validator: a.validator,
        epoch: a.body.validator_epoch,
        attestation_a: Some(a.clone()),
        attestation_b: Some(b.clone()),
        note_hash: Hash::hash_borsh(&(a.body.checkpoint_id(), b.body.checkpoint_id())),
    })
}

#[cfg(test)]
mod tests {
    use agora_types::{CheckpointBody, Hash, NativeAssetId};

    use super::*;

    fn att(set: NativeAssetId, score: u64, block: u8) -> CheckpointAttestation {
        CheckpointAttestation {
            body: CheckpointBody {
                chain_id: "c".into(),
                genesis_hash: Hash::ZERO,
                consensus_policy_hash: Hash::ZERO,
                state_transition_version: "v".into(),
                blue_score: score,
                block_hash: Hash([block; 32]),
                state_root: Hash::ZERO,
                validator_epoch: 7,
            },
            set,
            validator: Address([9u8; 20]),
            public_key: vec![],
            signature: vec![],
        }
    }

    #[test]
    fn slash_defaults_conservative() {
        let p = SlashPolicy::default();
        let (amt, tomb, _) = p.penalty_for(&EvidenceKind::DoubleCheckpointSignature, 10_000);
        assert_eq!(amt, 500); // 5%
        assert!(tomb);
        let (amt, tomb, jail) = p.penalty_for(&EvidenceKind::InvalidStateAttestation, 10_000);
        assert_eq!(amt, 100); // 1%
        assert!(!tomb);
        assert_eq!(jail, 1);
        let (amt, _, _) = p.penalty_for(&EvidenceKind::ExtendedDowntime, 10_000);
        assert_eq!(amt, 0);
    }

    #[test]
    fn detects_equivocation() {
        let a = att(NativeAssetId::OVL, 10, 1);
        let b = att(NativeAssetId::OVL, 10, 2);
        let ev = detect_double_checkpoint(&a, &b).unwrap();
        assert_eq!(ev.kind, EvidenceKind::DoubleCheckpointSignature);
        assert!(detect_double_checkpoint(&a, &a).is_none());
    }
}
