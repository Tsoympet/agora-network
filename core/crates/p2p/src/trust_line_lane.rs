//! Mempool admission for DRC trust line set and issued-value transfer lanes.

use agora_types::{
    drc_issued_transfer_mutation_meta_keys, drc_trust_line_set_mutation_meta_keys, Address,
    DrcIssuedTransferTx, DrcTrustLineSetTx, Hash, DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION,
    DRC_TRUST_LINE_SET_TICKET_VERSION,
};

use crate::P2pError;

use super::Mempool;

type LineSlot = (Address, Hash);

impl Mempool {
    pub fn pending_trust_line_set(&self, holder: &Address, asset_key: &Hash) -> bool {
        self.pending_trust_line_set_slots
            .contains(&(*holder, *asset_key))
    }

    pub fn pending_trust_line_delete(&self, holder: &Address, asset_key: &Hash) -> bool {
        self.pending_trust_line_delete_slots
            .contains(&(*holder, *asset_key))
    }

    pub fn pending_trust_line_create(&self, holder: &Address, asset_key: &Hash) -> bool {
        self.pending_trust_line_create_slots
            .contains(&(*holder, *asset_key))
    }

    pub fn trust_line_mutation_reserved(&self, holder: &Address, asset_key: &Hash) -> bool {
        self.reserved_trust_line_set_slots
            .contains_key(&(*holder, *asset_key))
    }

    pub fn issuer_liability_mutation_reserved(&self, asset_key: &Hash) -> bool {
        self.reserved_issuer_liability_assets
            .contains_key(asset_key)
    }

    pub fn trust_line_pending_balance_delta(&self, holder: &Address, asset_key: &Hash) -> i128 {
        self.pending_trust_line_balance_delta
            .get(&(*holder, *asset_key))
            .copied()
            .unwrap_or(0)
    }

    pub fn trust_line_meta_key_reserved(&self, key: &[u8]) -> bool {
        self.reserved_trust_line_meta_keys.contains_key(key)
    }

    pub fn trust_line_meta_key_owner_tx(&self, key: &[u8]) -> Option<Hash> {
        self.reserved_trust_line_meta_keys.get(key).copied()
    }

    pub fn revalidate_pending_trust_line_transfers<F>(&mut self, mut still_valid: F)
    where
        F: FnMut(&DrcIssuedTransferTx) -> bool,
    {
        let stale: Vec<Hash> = self
            .drc_issued_transfer_txs
            .iter()
            .filter_map(|(id, tx)| (!still_valid(tx)).then_some(*id))
            .collect();
        for id in stale {
            let _ = self.remove_drc_issued_transfer(&id);
        }
    }

    pub fn pending_issued_transfer_txs(&self) -> Vec<DrcIssuedTransferTx> {
        self.drc_issued_transfer_txs.values().cloned().collect()
    }

    fn reserve_mutation_meta_keys(
        &mut self,
        tx_id: Hash,
        keys: Vec<Vec<u8>>,
    ) -> Result<(), P2pError> {
        for key in keys {
            if let Some(existing) = self.reserved_trust_line_meta_keys.get(&key) {
                if *existing != tx_id {
                    return Err(P2pError::MempoolRejected(
                        "trust line meta mutation already reserved".into(),
                    ));
                }
                continue;
            }
            self.reserved_trust_line_meta_keys.insert(key, tx_id);
        }
        Ok(())
    }

    fn release_mutation_meta_keys_for_tx(&mut self, tx_id: &Hash) {
        self.reserved_trust_line_meta_keys
            .retain(|_, owner| owner != tx_id);
    }

    pub fn admit_drc_trust_line_set(
        &mut self,
        tx: DrcTrustLineSetTx,
        pending_create: bool,
        pending_delete: bool,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.trust_line_set_tx_id();
        if self.drc_trust_line_set_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let asset_key = tx.asset_id().asset_key();
        let slot = (tx.holder, asset_key);
        if self.reserved_trust_line_set_slots.contains_key(&slot) {
            return Err(P2pError::MempoolRejected(
                "trust line already has a pending set or delete".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.holder,
            tx.version,
            DRC_TRUST_LINE_SET_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_trust_line_set_slot(slot, id, pending_create, pending_delete)?;
        let meta_keys = drc_trust_line_set_mutation_meta_keys(&tx);
        if let Err(e) = self.reserve_mutation_meta_keys(id, meta_keys) {
            self.release_trust_line_set_slot(&slot);
            return Err(e);
        }
        if let Err(e) = self.reserve_drc_slot(id, tx.holder, reservation) {
            self.release_mutation_meta_keys_for_tx(&id);
            self.release_trust_line_set_slot(&slot);
            return Err(e);
        }
        self.drc_trust_line_set_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_issued_transfer(&mut self, tx: DrcIssuedTransferTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.issued_transfer_tx_id();
        if self.drc_issued_transfer_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let asset = tx.asset_id();
        let asset_key = asset.asset_key();
        let issuer = asset.issuer;

        if self.pending_trust_line_delete(&tx.sender, &asset_key)
            || (tx.recipient != tx.sender
                && self.pending_trust_line_delete(&tx.recipient, &asset_key))
        {
            return Err(P2pError::MempoolRejected(
                "mempool rejects transfer while trust line delete is pending (same-block is consensus-only)"
                    .into(),
            ));
        }

        if tx.sender != issuer && self.pending_trust_line_create(&tx.sender, &asset_key) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects transfer while sender trust line create is pending".into(),
            ));
        }
        if tx.recipient != issuer && self.pending_trust_line_create(&tx.recipient, &asset_key) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects transfer while recipient trust line create is pending".into(),
            ));
        }

        if self.asset_policy_mutation_reserved(&asset_key) {
            return Err(P2pError::MempoolRejected(
                "asset policy mutation pending".into(),
            ));
        }

        if tx.sender != issuer && self.issuer_control_mutation_reserved(&tx.sender, &asset_key) {
            return Err(P2pError::MempoolRejected(
                "issuer control pending on sender line".into(),
            ));
        }
        if tx.recipient != issuer
            && tx.recipient != tx.sender
            && self.issuer_control_mutation_reserved(&tx.recipient, &asset_key)
        {
            return Err(P2pError::MempoolRejected(
                "issuer control pending on recipient line".into(),
            ));
        }

        let touches_liability = tx.sender == issuer || tx.recipient == issuer;
        if touches_liability
            && self
                .reserved_issuer_liability_assets
                .contains_key(&asset_key)
        {
            return Err(P2pError::MempoolRejected(
                "issuer liability already has a pending issue or redeem".into(),
            ));
        }

        let reservation = Self::drc_sender_reservation_from_selector(
            tx.sender,
            tx.version,
            DRC_TRUST_LINE_ISSUED_TRANSFER_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;

        if touches_liability {
            self.reserved_issuer_liability_assets.insert(asset_key, id);
        }

        let meta_keys = drc_issued_transfer_mutation_meta_keys(&tx);
        if let Err(e) = self.reserve_mutation_meta_keys(id, meta_keys) {
            if touches_liability {
                self.reserved_issuer_liability_assets.remove(&asset_key);
            }
            return Err(e);
        }

        if let Err(e) = self.apply_pending_transfer_deltas(&tx, asset_key, issuer) {
            self.release_mutation_meta_keys_for_tx(&id);
            if touches_liability {
                self.reserved_issuer_liability_assets.remove(&asset_key);
            }
            return Err(e);
        }

        if let Err(e) = self.reserve_drc_slot(id, tx.sender, reservation) {
            self.rollback_pending_transfer_deltas(&tx, asset_key, issuer);
            self.release_mutation_meta_keys_for_tx(&id);
            if touches_liability {
                self.reserved_issuer_liability_assets.remove(&asset_key);
            }
            return Err(e);
        }

        self.drc_issued_transfer_txs.insert(id, tx);
        Ok(id)
    }

    fn apply_pending_transfer_deltas(
        &mut self,
        tx: &DrcIssuedTransferTx,
        asset_key: Hash,
        issuer: Address,
    ) -> Result<(), P2pError> {
        let amount = tx.amount.as_units() as i128;
        if tx.sender == issuer && tx.recipient != issuer {
            self.bump_pending_delta(tx.recipient, asset_key, amount)?;
        } else if tx.recipient == issuer && tx.sender != issuer {
            self.bump_pending_delta(tx.sender, asset_key, -amount)?;
        } else if tx.sender != issuer && tx.recipient != issuer {
            self.bump_pending_delta(tx.sender, asset_key, -amount)?;
            self.bump_pending_delta(tx.recipient, asset_key, amount)?;
        }
        Ok(())
    }

    fn rollback_pending_transfer_deltas(
        &mut self,
        tx: &DrcIssuedTransferTx,
        asset_key: Hash,
        issuer: Address,
    ) {
        let amount = tx.amount.as_units() as i128;
        if tx.sender == issuer && tx.recipient != issuer {
            self.bump_pending_delta_unchecked(tx.recipient, asset_key, -amount);
        } else if tx.recipient == issuer && tx.sender != issuer {
            self.bump_pending_delta_unchecked(tx.sender, asset_key, amount);
        } else if tx.sender != issuer && tx.recipient != issuer {
            self.bump_pending_delta_unchecked(tx.sender, asset_key, amount);
            self.bump_pending_delta_unchecked(tx.recipient, asset_key, -amount);
        }
    }

    fn bump_pending_delta(
        &mut self,
        holder: Address,
        asset_key: Hash,
        delta: i128,
    ) -> Result<(), P2pError> {
        let slot = (holder, asset_key);
        let next = self
            .pending_trust_line_balance_delta
            .get(&slot)
            .copied()
            .unwrap_or(0)
            .checked_add(delta)
            .ok_or_else(|| {
                P2pError::MempoolRejected("pending trust line balance overflow".into())
            })?;
        if next == 0 {
            self.pending_trust_line_balance_delta.remove(&slot);
        } else {
            self.pending_trust_line_balance_delta.insert(slot, next);
        }
        Ok(())
    }

    fn bump_pending_delta_unchecked(&mut self, holder: Address, asset_key: Hash, delta: i128) {
        let slot = (holder, asset_key);
        let next = self
            .pending_trust_line_balance_delta
            .get(&slot)
            .copied()
            .unwrap_or(0)
            .saturating_add(delta);
        if next == 0 {
            self.pending_trust_line_balance_delta.remove(&slot);
        } else {
            self.pending_trust_line_balance_delta.insert(slot, next);
        }
    }

    fn reserve_trust_line_set_slot(
        &mut self,
        slot: LineSlot,
        tx_id: Hash,
        pending_create: bool,
        pending_delete: bool,
    ) -> Result<(), P2pError> {
        if self.reserved_trust_line_set_slots.contains_key(&slot) {
            return Err(P2pError::MempoolRejected(
                "trust line already has a pending set".into(),
            ));
        }
        self.reserved_trust_line_set_slots.insert(slot, tx_id);
        self.pending_trust_line_set_slots.insert(slot);
        if pending_delete {
            self.pending_trust_line_delete_slots.insert(slot);
        }
        if pending_create {
            self.pending_trust_line_create_slots.insert(slot);
        }
        Ok(())
    }

    fn release_trust_line_set_slot(&mut self, slot: &LineSlot) {
        self.reserved_trust_line_set_slots.remove(slot);
        self.pending_trust_line_set_slots.remove(slot);
        self.pending_trust_line_delete_slots.remove(slot);
        self.pending_trust_line_create_slots.remove(slot);
    }

    pub fn remove_drc_trust_line_set(&mut self, id: &Hash) -> Option<DrcTrustLineSetTx> {
        let tx = self.drc_trust_line_set_txs.remove(id)?;
        let asset_key = tx.asset_id().asset_key();
        let slot = (tx.holder, asset_key);
        self.release_trust_line_set_slot(&slot);
        self.release_mutation_meta_keys_for_tx(id);
        self.release_drc_slot(id, tx.holder);
        Some(tx)
    }

    pub fn remove_drc_issued_transfer(&mut self, id: &Hash) -> Option<DrcIssuedTransferTx> {
        let tx = self.drc_issued_transfer_txs.remove(id)?;
        let asset_key = tx.asset_id().asset_key();
        let issuer = tx.asset_id().issuer;
        self.rollback_pending_transfer_deltas(&tx, asset_key, issuer);
        self.reserved_issuer_liability_assets.remove(&asset_key);
        self.release_mutation_meta_keys_for_tx(id);
        self.release_drc_slot(id, tx.sender);
        Some(tx)
    }

    pub fn select_drc_trust_line_sets(&self, max: usize) -> Vec<DrcTrustLineSetTx> {
        let mut txs: Vec<_> = self.drc_trust_line_set_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            a.trust_line_set_tx_id()
                .as_bytes()
                .cmp(b.trust_line_set_tx_id().as_bytes())
        });
        txs.truncate(max);
        txs
    }

    pub fn select_drc_issued_transfers(&self, max: usize) -> Vec<DrcIssuedTransferTx> {
        let mut txs: Vec<_> = self.drc_issued_transfer_txs.values().cloned().collect();
        txs.sort_by(|a, b| {
            a.issued_transfer_tx_id()
                .as_bytes()
                .cmp(b.issued_transfer_tx_id().as_bytes())
        });
        txs.truncate(max);
        txs
    }

    pub(super) fn trust_line_maps_len(&self) -> usize {
        self.drc_trust_line_set_txs.len() + self.drc_issued_transfer_txs.len()
    }

    pub(super) fn trust_line_maps_contains(&self, tx_id: &Hash) -> bool {
        self.drc_trust_line_set_txs.contains_key(tx_id)
            || self.drc_issued_transfer_txs.contains_key(tx_id)
    }

    pub(super) fn evict_trust_line_lanes_from_block(&mut self, block: &agora_types::Block) {
        for tx in &block.drc_trust_line_sets {
            let id = tx.trust_line_set_tx_id();
            let _ = self.remove_drc_trust_line_set(&id);
        }
        for tx in &block.drc_issued_transfers {
            let id = tx.issued_transfer_tx_id();
            let _ = self.remove_drc_issued_transfer(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_types::{Amount, IssuedAmount, IssuedCurrencyCode};

    use super::*;

    fn std_usd() -> IssuedCurrencyCode {
        IssuedCurrencyCode(*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0")
    }

    #[test]
    fn pending_delete_blocks_transfer_publicly() {
        let mut pool = Mempool::new(32);
        let holder = Address([1u8; 20]);
        let issuer = Address([2u8; 20]);
        let recipient = Address([3u8; 20]);
        let currency = std_usd();
        let asset_key = agora_types::IssuedAssetId { issuer, currency }.asset_key();

        let delete = DrcTrustLineSetTx {
            version: agora_types::DRC_TRUST_LINE_SET_TX_VERSION,
            holder,
            issuer,
            currency,
            limit: IssuedAmount::ZERO,
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        };
        pool.admit_drc_trust_line_set(delete, false, true).unwrap();
        assert!(pool.pending_trust_line_delete(&holder, &asset_key));

        let transfer = DrcIssuedTransferTx {
            version: agora_types::DRC_TRUST_LINE_ISSUED_TRANSFER_TX_VERSION,
            sender: holder,
            recipient,
            issuer,
            currency,
            amount: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            destination_tag: None,
            source_tag: None,
            invoice_id: Hash::ZERO,
            nonce: 1,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        };
        assert!(pool.admit_drc_issued_transfer(transfer).is_err());
    }
}
