//! Mempool admission for native DRC OfferCreate / OfferCancel.
//!
//! Sequence and ticket slots follow the other DRC lanes. Native sell amounts and
//! issued-holder sell amounts are summed so two pending offers cannot reserve the
//! same units. Consensus apply remains the funding authority.

use agora_types::{
    Address, DrcBookAsset, DrcOfferCancelTx, DrcOfferCreateTx, Hash,
    DRC_OFFER_CANCEL_TICKET_VERSION, DRC_OFFER_CREATE_TICKET_VERSION,
};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn pending_native_offer_lock(&self, owner: &Address) -> u64 {
        self.pending_native_offer_lock
            .get(owner)
            .copied()
            .unwrap_or(0)
    }

    pub fn pending_issued_offer_reserve(&self, holder: &Address, asset_key: &Hash) -> u64 {
        self.pending_issued_offer_reserve
            .get(&(*holder, *asset_key))
            .copied()
            .unwrap_or(0)
    }

    pub fn offer_cancel_reserved(&self, offer_id: &Hash) -> bool {
        self.pending_offer_cancels.contains_key(offer_id)
    }

    pub fn pending_offer_creates(&self) -> Vec<DrcOfferCreateTx> {
        self.drc_offer_create_txs.values().cloned().collect()
    }

    pub fn pending_offer_cancels(&self) -> Vec<DrcOfferCancelTx> {
        self.drc_offer_cancel_txs.values().cloned().collect()
    }

    pub(super) fn offer_maps_len(&self) -> usize {
        self.drc_offer_create_txs.len() + self.drc_offer_cancel_txs.len()
    }

    pub(super) fn offer_maps_contains(&self, tx_id: &Hash) -> bool {
        self.drc_offer_create_txs.contains_key(tx_id)
            || self.drc_offer_cancel_txs.contains_key(tx_id)
    }

    fn add_native_lock(&mut self, owner: Address, amount: u64) -> Result<(), P2pError> {
        if amount == 0 {
            return Ok(());
        }
        let entry = self.pending_native_offer_lock.entry(owner).or_insert(0);
        *entry = entry.checked_add(amount).ok_or_else(|| {
            P2pError::MempoolRejected("DRC offer native reservation overflow".into())
        })?;
        Ok(())
    }

    fn sub_native_lock(&mut self, owner: Address, amount: u64) {
        if amount == 0 {
            return;
        }
        let Some(entry) = self.pending_native_offer_lock.get_mut(&owner) else {
            return;
        };
        *entry = entry.saturating_sub(amount);
        if *entry == 0 {
            self.pending_native_offer_lock.remove(&owner);
        }
    }

    fn add_issued_reserve(
        &mut self,
        holder: Address,
        asset_key: Hash,
        amount: u64,
    ) -> Result<(), P2pError> {
        if amount == 0 {
            return Ok(());
        }
        let entry = self
            .pending_issued_offer_reserve
            .entry((holder, asset_key))
            .or_insert(0);
        *entry = entry.checked_add(amount).ok_or_else(|| {
            P2pError::MempoolRejected("DRC offer issued reservation overflow".into())
        })?;
        Ok(())
    }

    fn sub_issued_reserve(&mut self, holder: Address, asset_key: Hash, amount: u64) {
        if amount == 0 {
            return;
        }
        let Some(entry) = self
            .pending_issued_offer_reserve
            .get_mut(&(holder, asset_key))
        else {
            return;
        };
        *entry = entry.saturating_sub(amount);
        if *entry == 0 {
            self.pending_issued_offer_reserve
                .remove(&(holder, asset_key));
        }
    }

    fn release_offer_create(&mut self, tx: &DrcOfferCreateTx) {
        self.release_drc_slot(&tx.offer_id(), tx.owner);
        let mut native = tx.fee.as_base_units();
        if tx.taker_gets.is_native() {
            native = native.saturating_add(tx.taker_gets_amount);
        }
        self.sub_native_lock(tx.owner, native);
        if let DrcBookAsset::Issued(asset) = tx.taker_gets {
            if asset.issuer != tx.owner {
                self.sub_issued_reserve(tx.owner, asset.asset_key(), tx.taker_gets_amount);
            }
        }
    }

    pub fn admit_drc_offer_create(&mut self, tx: DrcOfferCreateTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|error| P2pError::MempoolRejected(error.to_string()))?;
        let id = tx.offer_id();
        if self.drc_offer_create_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_OFFER_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
        let mut native = tx.fee.as_base_units();
        if tx.taker_gets.is_native() {
            native = native.saturating_add(tx.taker_gets_amount);
        }
        if let Err(error) = self.add_native_lock(tx.owner, native) {
            self.release_drc_slot(&id, tx.owner);
            return Err(error);
        }
        if let DrcBookAsset::Issued(asset) = tx.taker_gets {
            if asset.issuer != tx.owner {
                if let Err(error) =
                    self.add_issued_reserve(tx.owner, asset.asset_key(), tx.taker_gets_amount)
                {
                    self.sub_native_lock(tx.owner, native);
                    self.release_drc_slot(&id, tx.owner);
                    return Err(error);
                }
            }
        }
        self.drc_offer_create_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_offer_cancel(&mut self, tx: DrcOfferCancelTx) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|error| P2pError::MempoolRejected(error.to_string()))?;
        let id = tx.cancel_tx_id();
        if self.drc_offer_cancel_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.drc_offer_create_txs.contains_key(&tx.offer_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects offer cancel while create is pending".into(),
            ));
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.pending_offer_cancels.contains_key(&tx.offer_id) {
            return Err(P2pError::MempoolRejected(
                "DRC offer already has a pending cancel".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_OFFER_CANCEL_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.pending_offer_cancels.insert(tx.offer_id, id);
        if let Err(error) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.pending_offer_cancels.remove(&tx.offer_id);
            return Err(error);
        }
        if let Err(error) = self.add_native_lock(tx.submitter, tx.fee.as_base_units()) {
            self.release_drc_slot(&id, tx.submitter);
            self.pending_offer_cancels.remove(&tx.offer_id);
            return Err(error);
        }
        self.drc_offer_cancel_txs.insert(id, tx);
        Ok(id)
    }

    pub fn select_drc_offer_creates(&self, max: usize) -> Vec<DrcOfferCreateTx> {
        let mut txs: Vec<_> = self.drc_offer_create_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.offer_id().as_bytes().cmp(b.offer_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_offer_cancels(&self, max: usize) -> Vec<DrcOfferCancelTx> {
        let mut txs: Vec<_> = self.drc_offer_cancel_txs.values().cloned().collect();
        txs.sort_by(|a, b| a.cancel_tx_id().as_bytes().cmp(b.cancel_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn remove_drc_offer_create(&mut self, id: &Hash) -> bool {
        let Some(tx) = self.drc_offer_create_txs.remove(id) else {
            return false;
        };
        self.release_offer_create(&tx);
        true
    }

    pub fn remove_drc_offer_cancel(&mut self, id: &Hash) -> bool {
        let Some(tx) = self.drc_offer_cancel_txs.remove(id) else {
            return false;
        };
        self.release_drc_slot(id, tx.submitter);
        self.pending_offer_cancels.remove(&tx.offer_id);
        self.sub_native_lock(tx.submitter, tx.fee.as_base_units());
        true
    }

    pub(crate) fn evict_offer_lanes_from_block(&mut self, block: &agora_types::Block) {
        for tx in &block.drc_offer_creates {
            let _ = self.remove_drc_offer_create(&tx.offer_id());
        }
        for tx in &block.drc_offer_cancels {
            let _ = self.remove_drc_offer_cancel(&tx.cancel_tx_id());
        }
    }
}
