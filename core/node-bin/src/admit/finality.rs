//! Trident dual-PoS finality hooks on [`super::ChainState`].
//!
//! Certificate PoW progress runs after successful admit. Attestations are admitted
//! via [`ChainState::admit_attestation`]. Reorgs that abandon a finalized blue are
//! rejected before durable UTXO mutation.

use agora_consensus::{
    assert_reorg_allowed, detect_double_checkpoint, note_pow_progress, note_signed_stake,
    refresh_certificate, FinalityPowPolicy, SlashPolicy,
};
use agora_crypto::verify_checkpoint_attestation;
use agora_state_machine::{
    apply_evidence, build_snapshot, compose_trident_state_root, load_attestation_index,
    load_certificate, load_finalized_blue_score, load_last_attestation, put_attestation_index_into,
    put_certificate_into, put_last_attestation_into, signed_stake_for, validator_key_matches,
    WriteBatch, TRIDENT_STATE_TRANSITION_VERSION,
};
use agora_types::{
    CheckpointAttestation, CheckpointBody, FinalityCertificate, Hash, NativeAssetId,
};

use super::{common_prefix_len, AdmitError, ChainState};

impl ChainState {
    /// Reject tip changes that abandon any blue at or below the finalized frontier.
    pub(crate) fn guard_reorg_vs_finality(
        &self,
        old_virtual: Hash,
        new_virtual: Hash,
    ) -> Result<(), AdmitError> {
        let finalized = load_finalized_blue_score(self.store.as_ref())
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        let Some(finalized) = finalized else {
            return Ok(());
        };
        if old_virtual == new_virtual {
            return Ok(());
        }
        let applied = self.applied_blues(old_virtual)?;
        let target = self.applied_blues(new_virtual)?;
        let prefix = common_prefix_len(&applied, &target);
        for hash in &applied[prefix..] {
            let score = self.ghostdag.blue_score(hash).unwrap_or(0);
            if score <= finalized {
                return Err(AdmitError::FinalityReorg {
                    finalized,
                    abandoned: score,
                });
            }
        }
        let new_score = self.ghostdag.blue_score(&new_virtual).unwrap_or(0);
        assert_reorg_allowed(Some(finalized), new_score).map_err(|_| {
            AdmitError::FinalityReorg {
                finalized,
                abandoned: new_score,
            }
        })?;
        Ok(())
    }

    /// After a successful admit, note PoW progress on the virtual tip certificate.
    pub(crate) fn note_pow_on_virtual_tip(&self, tip: Hash) -> Result<(), AdmitError> {
        let Some(score) = self.ghostdag.blue_score(&tip) else {
            return Ok(());
        };
        let body = self.checkpoint_body_for(tip, score)?;
        let mut cert = load_certificate(self.store.as_ref(), &tip)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
            .unwrap_or_else(|| FinalityCertificate::new(body.clone()));
        if cert.body != body {
            // Keep PoS counters if only non-identity fields drifted; otherwise reset.
            let prev_ovl = (cert.ovl_signed_stake, cert.ovl_active_stake);
            let prev_drc = (cert.drc_signed_stake, cert.drc_active_stake);
            cert = FinalityCertificate::new(body);
            cert.ovl_signed_stake = prev_ovl.0;
            cert.ovl_active_stake = prev_ovl.1;
            cert.drc_signed_stake = prev_drc.0;
            cert.drc_active_stake = prev_drc.1;
        }
        note_pow_progress(&mut cert, score, &FinalityPowPolicy::default());
        refresh_certificate(&mut cert);
        let mut batch = WriteBatch::new();
        put_certificate_into(&mut batch, &cert).map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .write_batch(batch)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Admit one checkpoint attestation (RPC / gossip).
    pub fn admit_attestation(
        &mut self,
        att: CheckpointAttestation,
    ) -> Result<FinalityCertificate, AdmitError> {
        if !matches!(att.set, NativeAssetId::OVL | NativeAssetId::DRC) {
            return Err(AdmitError::InvalidAttestation(
                "attestation set must be OVL or DRC".into(),
            ));
        }
        verify_checkpoint_attestation(&att)
            .map_err(|e| AdmitError::InvalidAttestation(format!("signature: {e}")))?;
        if !validator_key_matches(self.store.as_ref(), &att)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
        {
            return Err(AdmitError::InvalidAttestation(
                "validator key mismatch or inactive".into(),
            ));
        }

        if let Some(prev) = load_last_attestation(
            self.store.as_ref(),
            att.set,
            &att.validator,
            att.body.validator_epoch,
        )
        .map_err(|e| AdmitError::Storage(e.to_string()))?
        {
            if let Some(ev) = detect_double_checkpoint(&prev, &att) {
                let mut batch = WriteBatch::new();
                apply_evidence(
                    self.store.as_ref(),
                    &mut batch,
                    &ev,
                    &SlashPolicy::default(),
                )
                .map_err(|e| AdmitError::InvalidAttestation(e.to_string()))?;
                put_last_attestation_into(&mut batch, &att)
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
                self.store
                    .write_batch(batch)
                    .map_err(|e| AdmitError::Storage(e.to_string()))?;
                return Err(AdmitError::InvalidAttestation(
                    "equivocation evidence applied".into(),
                ));
            }
            if prev.body.checkpoint_id() == att.body.checkpoint_id() {
                return load_certificate(self.store.as_ref(), &att.body.block_hash)
                    .map_err(|e| AdmitError::Storage(e.to_string()))?
                    .ok_or_else(|| {
                        AdmitError::InvalidAttestation("missing certificate for replay".into())
                    });
            }
        }

        let Some(local_score) = self.ghostdag.blue_score(&att.body.block_hash) else {
            return Err(AdmitError::InvalidAttestation(
                "unknown checkpoint block".into(),
            ));
        };
        let expected = self.checkpoint_body_for(att.body.block_hash, local_score)?;
        if att.body != expected {
            return Err(AdmitError::InvalidAttestation(
                "checkpoint body mismatch".into(),
            ));
        }

        let mut idx = load_attestation_index(self.store.as_ref(), &att.body.block_hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        idx.insert(att.set, att.validator);

        let epoch = att.body.validator_epoch;
        let snap = build_snapshot(self.store.as_ref(), att.set, epoch)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        let signed = signed_stake_for(&snap, idx.signers(att.set));
        let active = snap.total_active_stake;

        let mut cert = load_certificate(self.store.as_ref(), &att.body.block_hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?
            .unwrap_or_else(|| FinalityCertificate::new(expected.clone()));
        if cert.body != expected {
            cert = FinalityCertificate::new(expected.clone());
        }
        note_pow_progress(&mut cert, local_score, &FinalityPowPolicy::default());
        note_signed_stake(
            &mut cert,
            &expected,
            att.set == NativeAssetId::OVL,
            signed,
            active,
        )
        .map_err(|e| AdmitError::InvalidAttestation(e.to_string()))?;

        let mut batch = WriteBatch::new();
        put_attestation_index_into(&mut batch, &att.body.block_hash, &idx)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        put_last_attestation_into(&mut batch, &att)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        put_certificate_into(&mut batch, &cert).map_err(|e| AdmitError::Storage(e.to_string()))?;
        self.store
            .write_batch(batch)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        Ok(cert)
    }

    pub fn finalized_blue_score(&self) -> Result<Option<u64>, AdmitError> {
        load_finalized_blue_score(self.store.as_ref())
            .map_err(|e| AdmitError::Storage(e.to_string()))
    }

    pub fn finality_certificate(
        &self,
        block_hash: &Hash,
    ) -> Result<Option<FinalityCertificate>, AdmitError> {
        load_certificate(self.store.as_ref(), block_hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))
    }

    pub(crate) fn checkpoint_body_for(
        &self,
        block_hash: Hash,
        blue_score: u64,
    ) -> Result<CheckpointBody, AdmitError> {
        let chain_id = self
            .auth
            .as_ref()
            .map(|a| a.chain_id.clone())
            .unwrap_or_else(|| "agora-dev".into());
        let epoch_ovl = agora_state_machine::load_epoch(self.store.as_ref(), NativeAssetId::OVL)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        let epoch_drc = agora_state_machine::load_epoch(self.store.as_ref(), NativeAssetId::DRC)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        let validator_epoch = epoch_ovl.max(epoch_drc);
        let state_root = compose_trident_state_root(self.store.as_ref(), &block_hash)
            .map_err(|e| AdmitError::Storage(e.to_string()))?;
        Ok(CheckpointBody {
            chain_id,
            genesis_hash: self.genesis,
            consensus_policy_hash: self.consensus_policy_hash,
            state_transition_version: TRIDENT_STATE_TRANSITION_VERSION.into(),
            blue_score,
            block_hash,
            state_root,
            validator_epoch,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agora_consensus::PowAlgorithm;
    use agora_state_machine::{put_certificate_into, GenesisBuilder, StateStore, WriteBatch};
    use agora_types::{CheckpointState, FinalityCertificate, Hash};

    use crate::admit::{ChainBootConfig, ChainState};
    use crate::storage_policy::StoragePolicy;

    #[test]
    fn reorg_guard_allows_same_tip_when_finalized() {
        let store = Arc::new(StateStore::open_in_memory());
        let genesis_block = GenesisBuilder::default().build_block();
        let genesis = genesis_block.id();
        GenesisBuilder::default()
            .ignite(store.as_ref())
            .expect("ignite");
        let mut boot = ChainBootConfig::default();
        boot.pow = PowAlgorithm::RandomX;
        boot.initial_bits = 0;
        boot.daa.min_level = 0;
        boot.chain_id = "agora-dev".into();
        let chain =
            ChainState::bootstrap_with(store.clone(), genesis, boot, StoragePolicy::default())
                .unwrap();

        let mut cert = FinalityCertificate::new(agora_types::CheckpointBody {
            chain_id: "agora-dev".into(),
            genesis_hash: genesis,
            consensus_policy_hash: Hash::ZERO,
            state_transition_version: "v".into(),
            blue_score: 0,
            block_hash: genesis,
            state_root: Hash::ZERO,
            validator_epoch: 0,
        });
        cert.state = CheckpointState::Finalized;
        cert.pow_work_met = true;
        cert.ovl_signed_stake = 1;
        cert.ovl_active_stake = 1;
        cert.drc_signed_stake = 1;
        cert.drc_active_stake = 1;
        let mut batch = WriteBatch::new();
        put_certificate_into(&mut batch, &cert).unwrap();
        store.write_batch(batch).unwrap();

        assert!(chain.guard_reorg_vs_finality(genesis, genesis).is_ok());
    }
}
