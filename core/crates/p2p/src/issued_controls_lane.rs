//! Mempool admission for issued-asset policy, line issuer control, and clawback lanes.

use agora_types::{
    drc_issued_asset_policy_set_mutation_meta_keys, drc_issued_clawback_mutation_meta_keys,
    drc_trust_line_issuer_control_mutation_meta_keys, Address, DrcIssuedAssetPolicySetTx,
    DrcIssuedClawbackTx, DrcTrustLineIssuerControlTx, Hash,
    DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION, DRC_ISSUED_CLAWBACK_TICKET_VERSION,
    DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION,
};

use crate::P2pError;

use super::Mempool;

type ControlSlot = (Address, Hash);

impl Mempool {
    pub fn asset_policy_mutation_reserved(&self, asset_key: &Hash) -> bool {
        self.reserved_asset_policy_assets.contains_key(asset_key)
    }

    pub fn issuer_control_mutation_reserved(&self, holder: &Address, asset_key: &Hash) -> bool {
        self.reserved_issuer_control_slots
            .contains_key(&(*holder, *asset_key))
    }

    pub fn pending_issued_clawback(&self, id: &Hash) -> bool {
        self.drc_issued_clawback_txs.contains_key(id)
    }

    fn reserve_meta_keys(&mut self, tx_id: Hash, keys: Vec<Vec<u8>>) -> Result<(), P2pError> {
        for key in keys {
            if let Some(existing) = self.reserved_trust_line_meta_keys.get(&key) {
                if *existing != tx_id {
                    return Err(P2pError::MempoolRejected(
                        "issued-control meta mutation already reserved".into(),
                    ));
                }
                continue;
            }
            self.reserved_trust_line_meta_keys.insert(key, tx_id);
        }
        Ok(())
    }

    fn release_meta_keys_for_tx(&mut self, tx_id: &Hash) {
        self.reserved_trust_line_meta_keys
            .retain(|_, owner| owner != tx_id);
    }

    pub fn admit_drc_issued_asset_policy_set(
        &mut self,
        tx: DrcIssuedAssetPolicySetTx,
        extra_meta_keys: Vec<Vec<u8>>,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.policy_set_tx_id();
        if self.drc_issued_asset_policy_set_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let asset_key = tx.asset_id().asset_key();
        if self.reserved_asset_policy_assets.contains_key(&asset_key) {
            return Err(P2pError::MempoolRejected(
                "asset policy already has a pending mutation".into(),
            ));
        }
        if self
            .reserved_issuer_liability_assets
            .contains_key(&asset_key)
        {
            return Err(P2pError::MempoolRejected(
                "issuer liability mutation pending on asset".into(),
            ));
        }

        let reservation = Self::drc_sender_reservation_from_selector(
            tx.issuer,
            tx.version,
            DRC_ISSUED_ASSET_POLICY_SET_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;

        self.reserved_asset_policy_assets.insert(asset_key, id);
        let mut meta_keys = drc_issued_asset_policy_set_mutation_meta_keys(&tx);
        meta_keys.extend(extra_meta_keys);
        if let Err(e) = self.reserve_meta_keys(id, meta_keys) {
            self.reserved_asset_policy_assets.remove(&asset_key);
            return Err(e);
        }
        if let Err(e) = self.reserve_drc_slot(id, tx.issuer, reservation) {
            self.release_meta_keys_for_tx(&id);
            self.reserved_asset_policy_assets.remove(&asset_key);
            return Err(e);
        }
        self.drc_issued_asset_policy_set_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_trust_line_issuer_control(
        &mut self,
        tx: DrcTrustLineIssuerControlTx,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.issuer_control_tx_id();
        if self.drc_trust_line_issuer_control_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let asset_key = tx.asset_id().asset_key();
        let slot: ControlSlot = (tx.holder, asset_key);
        if self.reserved_issuer_control_slots.contains_key(&slot) {
            return Err(P2pError::MempoolRejected(
                "issuer control already pending for line".into(),
            ));
        }
        if self.reserved_asset_policy_assets.contains_key(&asset_key) {
            return Err(P2pError::MempoolRejected(
                "asset policy mutation pending".into(),
            ));
        }
        if self.trust_line_mutation_reserved(&tx.holder, &asset_key) {
            return Err(P2pError::MempoolRejected(
                "trust line set/delete pending on line".into(),
            ));
        }

        let reservation = Self::drc_sender_reservation_from_selector(
            tx.issuer,
            tx.version,
            DRC_TRUST_LINE_ISSUER_CONTROL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;

        self.reserved_issuer_control_slots.insert(slot, id);
        let meta_keys = drc_trust_line_issuer_control_mutation_meta_keys(&tx);
        if let Err(e) = self.reserve_meta_keys(id, meta_keys) {
            self.reserved_issuer_control_slots.remove(&slot);
            return Err(e);
        }
        if let Err(e) = self.reserve_drc_slot(id, tx.issuer, reservation) {
            self.release_meta_keys_for_tx(&id);
            self.reserved_issuer_control_slots.remove(&slot);
            return Err(e);
        }
        self.drc_trust_line_issuer_control_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_issued_clawback(&mut self, tx: DrcIssuedClawbackTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.clawback_tx_id();
        if self.drc_issued_clawback_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let asset_key = tx.asset_id().asset_key();
        if self
            .reserved_issuer_liability_assets
            .contains_key(&asset_key)
        {
            return Err(P2pError::MempoolRejected(
                "issuer liability already has a pending mutation".into(),
            ));
        }
        if self.reserved_asset_policy_assets.contains_key(&asset_key) {
            return Err(P2pError::MempoolRejected(
                "asset policy mutation pending".into(),
            ));
        }
        if self.issuer_control_mutation_reserved(&tx.holder, &asset_key) {
            return Err(P2pError::MempoolRejected(
                "issuer control pending on holder line".into(),
            ));
        }

        let reservation = Self::drc_sender_reservation_from_selector(
            tx.issuer,
            tx.version,
            DRC_ISSUED_CLAWBACK_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;

        self.reserved_issuer_liability_assets.insert(asset_key, id);
        let meta_keys = drc_issued_clawback_mutation_meta_keys(&tx);
        if let Err(e) = self.reserve_meta_keys(id, meta_keys) {
            self.reserved_issuer_liability_assets.remove(&asset_key);
            return Err(e);
        }
        if let Err(e) = self.reserve_drc_slot(id, tx.issuer, reservation) {
            self.release_meta_keys_for_tx(&id);
            self.reserved_issuer_liability_assets.remove(&asset_key);
            return Err(e);
        }
        self.drc_issued_clawback_txs.insert(id, tx);
        Ok(id)
    }

    pub fn remove_drc_issued_asset_policy_set(
        &mut self,
        id: &Hash,
    ) -> Option<DrcIssuedAssetPolicySetTx> {
        let tx = self.drc_issued_asset_policy_set_txs.remove(id)?;
        let asset_key = tx.asset_id().asset_key();
        self.reserved_asset_policy_assets.remove(&asset_key);
        self.release_meta_keys_for_tx(id);
        self.release_drc_slot(id, tx.issuer);
        Some(tx)
    }

    pub fn remove_drc_trust_line_issuer_control(
        &mut self,
        id: &Hash,
    ) -> Option<DrcTrustLineIssuerControlTx> {
        let tx = self.drc_trust_line_issuer_control_txs.remove(id)?;
        let asset_key = tx.asset_id().asset_key();
        let slot = (tx.holder, asset_key);
        self.reserved_issuer_control_slots.remove(&slot);
        self.release_meta_keys_for_tx(id);
        self.release_drc_slot(id, tx.issuer);
        Some(tx)
    }

    pub fn remove_drc_issued_clawback(&mut self, id: &Hash) -> Option<DrcIssuedClawbackTx> {
        let tx = self.drc_issued_clawback_txs.remove(id)?;
        let asset_key = tx.asset_id().asset_key();
        self.reserved_issuer_liability_assets.remove(&asset_key);
        self.release_meta_keys_for_tx(id);
        self.release_drc_slot(id, tx.issuer);
        Some(tx)
    }

    pub fn pending_issued_asset_policy_set_txs(&self) -> Vec<DrcIssuedAssetPolicySetTx> {
        self.drc_issued_asset_policy_set_txs
            .values()
            .cloned()
            .collect()
    }

    pub fn pending_trust_line_issuer_control_txs(&self) -> Vec<DrcTrustLineIssuerControlTx> {
        self.drc_trust_line_issuer_control_txs
            .values()
            .cloned()
            .collect()
    }

    pub fn pending_issued_clawback_txs(&self) -> Vec<DrcIssuedClawbackTx> {
        self.drc_issued_clawback_txs.values().cloned().collect()
    }

    pub fn select_drc_issued_asset_policy_sets(
        &self,
        max: usize,
    ) -> Vec<DrcIssuedAssetPolicySetTx> {
        let mut txs: Vec<_> = self
            .drc_issued_asset_policy_set_txs
            .values()
            .cloned()
            .collect();
        txs.sort_by(|a, b| {
            a.policy_set_tx_id()
                .as_bytes()
                .cmp(b.policy_set_tx_id().as_bytes())
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_trust_line_issuer_controls(
        &self,
        max: usize,
    ) -> Vec<DrcTrustLineIssuerControlTx> {
        let mut txs: Vec<_> = self
            .drc_trust_line_issuer_control_txs
            .values()
            .cloned()
            .collect();
        txs.sort_by(|a, b| {
            a.issuer_control_tx_id()
                .as_bytes()
                .cmp(b.issuer_control_tx_id().as_bytes())
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_issued_clawbacks(&self, max: usize) -> Vec<DrcIssuedClawbackTx> {
        let mut txs: Vec<_> = self.drc_issued_clawback_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            a.clawback_tx_id()
                .as_bytes()
                .cmp(b.clawback_tx_id().as_bytes())
        });
        txs.truncate(max);
        txs
    }

    pub(super) fn issued_controls_maps_len(&self) -> usize {
        self.drc_issued_asset_policy_set_txs.len()
            + self.drc_trust_line_issuer_control_txs.len()
            + self.drc_issued_clawback_txs.len()
    }

    pub(super) fn issued_controls_maps_contains(&self, tx_id: &Hash) -> bool {
        self.drc_issued_asset_policy_set_txs.contains_key(tx_id)
            || self.drc_trust_line_issuer_control_txs.contains_key(tx_id)
            || self.drc_issued_clawback_txs.contains_key(tx_id)
    }

    pub(super) fn evict_issued_controls_lanes_from_block(&mut self, block: &agora_types::Block) {
        for tx in &block.drc_issued_asset_policy_sets {
            let id = tx.policy_set_tx_id();
            let _ = self.remove_drc_issued_asset_policy_set(&id);
        }
        for tx in &block.drc_trust_line_issuer_controls {
            let id = tx.issuer_control_tx_id();
            let _ = self.remove_drc_trust_line_issuer_control(&id);
        }
        for tx in &block.drc_issued_clawbacks {
            let id = tx.clawback_tx_id();
            let _ = self.remove_drc_issued_clawback(&id);
        }
    }
}
