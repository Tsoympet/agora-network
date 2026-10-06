//! Mempool admission for native DRC payment channel lanes (create/fund/claim/close).

use agora_types::{
    DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx,
    DrcPaymentChannelFundTx, Hash, DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION,
    DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION, DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION,
    DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn pending_payment_channel_create(&self, channel_id: &Hash) -> bool {
        self.pending_payment_channel_ids.contains(channel_id)
    }

    pub fn payment_channel_mutation_reserved(&self, channel_id: &Hash) -> bool {
        self.reserved_payment_channel_mutations
            .contains_key(channel_id)
    }

    fn reserve_payment_channel_mutation(
        &mut self,
        channel_id: Hash,
        tx_id: Hash,
    ) -> Result<(), P2pError> {
        if self
            .reserved_payment_channel_mutations
            .contains_key(&channel_id)
        {
            return Err(P2pError::MempoolRejected(
                "payment channel already has a pending fund, claim, or close".into(),
            ));
        }
        self.reserved_payment_channel_mutations
            .insert(channel_id, tx_id);
        Ok(())
    }

    fn release_payment_channel_mutation(&mut self, channel_id: &Hash) {
        self.reserved_payment_channel_mutations.remove(channel_id);
    }

    pub fn admit_drc_payment_channel_create(
        &mut self,
        tx: DrcPaymentChannelCreateTx,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        let id = tx.channel_id();
        if self.drc_payment_channel_create_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.owner,
            tx.version,
            DRC_PAYMENT_CHANNEL_CREATE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_drc_slot(id, tx.owner, reservation)?;
        self.pending_payment_channel_ids.insert(id);
        self.drc_payment_channel_create_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_payment_channel_fund(
        &mut self,
        tx: DrcPaymentChannelFundTx,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_payment_channel_ids.contains(&tx.channel_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects payment channel fund while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.fund_tx_id();
        if self.drc_payment_channel_fund_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_payment_channel_mutations
            .contains_key(&tx.channel_id)
        {
            return Err(P2pError::MempoolRejected(
                "payment channel already has a pending fund, claim, or close".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_PAYMENT_CHANNEL_FUND_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_payment_channel_mutation(tx.channel_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_payment_channel_mutation(&tx.channel_id);
            return Err(e);
        }
        self.drc_payment_channel_fund_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_payment_channel_claim(
        &mut self,
        tx: DrcPaymentChannelClaimTx,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_payment_channel_ids.contains(&tx.channel_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects payment channel claim while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.claim_tx_id();
        if self.drc_payment_channel_claim_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_payment_channel_mutations
            .contains_key(&tx.channel_id)
        {
            return Err(P2pError::MempoolRejected(
                "payment channel already has a pending fund, claim, or close".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_PAYMENT_CHANNEL_CLAIM_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_payment_channel_mutation(tx.channel_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_payment_channel_mutation(&tx.channel_id);
            return Err(e);
        }
        self.drc_payment_channel_claim_txs.insert(id, tx);
        Ok(id)
    }

    pub fn admit_drc_payment_channel_close(
        &mut self,
        tx: DrcPaymentChannelCloseTx,
    ) -> Result<Hash, P2pError> {
        tx.validate_structure()
            .map_err(|e| P2pError::MempoolRejected(e.to_string()))?;
        if self.pending_payment_channel_ids.contains(&tx.channel_id) {
            return Err(P2pError::MempoolRejected(
                "mempool rejects payment channel close while create is pending (same-block is consensus-only)"
                    .into(),
            ));
        }
        let id = tx.close_tx_id();
        if self.drc_payment_channel_close_txs.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_payment_channel_mutations
            .contains_key(&tx.channel_id)
        {
            return Err(P2pError::MempoolRejected(
                "payment channel already has a pending fund, claim, or close".into(),
            ));
        }
        let reservation = Self::drc_sender_reservation_from_selector(
            tx.submitter,
            tx.version,
            DRC_PAYMENT_CHANNEL_CLOSE_TICKET_VERSION,
            tx.nonce,
            tx.account_sequence,
        )?;
        self.reserve_payment_channel_mutation(tx.channel_id, id)?;
        if let Err(e) = self.reserve_drc_slot(id, tx.submitter, reservation) {
            self.release_payment_channel_mutation(&tx.channel_id);
            return Err(e);
        }
        self.drc_payment_channel_close_txs.insert(id, tx);
        Ok(id)
    }

    pub fn remove_drc_payment_channel_create(
        &mut self,
        id: &Hash,
    ) -> Option<DrcPaymentChannelCreateTx> {
        let tx = self.drc_payment_channel_create_txs.remove(id)?;
        self.pending_payment_channel_ids.remove(id);
        self.release_drc_slot(id, tx.owner);
        Some(tx)
    }

    pub fn remove_drc_payment_channel_fund(
        &mut self,
        id: &Hash,
    ) -> Option<DrcPaymentChannelFundTx> {
        let tx = self.drc_payment_channel_fund_txs.remove(id)?;
        self.release_payment_channel_mutation(&tx.channel_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn remove_drc_payment_channel_claim(
        &mut self,
        id: &Hash,
    ) -> Option<DrcPaymentChannelClaimTx> {
        let tx = self.drc_payment_channel_claim_txs.remove(id)?;
        self.release_payment_channel_mutation(&tx.channel_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn remove_drc_payment_channel_close(
        &mut self,
        id: &Hash,
    ) -> Option<DrcPaymentChannelCloseTx> {
        let tx = self.drc_payment_channel_close_txs.remove(id)?;
        self.release_payment_channel_mutation(&tx.channel_id);
        self.release_drc_slot(id, tx.submitter);
        Some(tx)
    }

    pub fn select_drc_payment_channel_creates(&self, max: usize) -> Vec<DrcPaymentChannelCreateTx> {
        let mut txs: Vec<_> = self
            .drc_payment_channel_create_txs
            .values()
            .cloned()
            .collect();
        txs.sort_by(|a, b| a.channel_id().as_bytes().cmp(b.channel_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_payment_channel_funds(&self, max: usize) -> Vec<DrcPaymentChannelFundTx> {
        let mut txs: Vec<_> = self
            .drc_payment_channel_fund_txs
            .values()
            .cloned()
            .collect();
        txs.sort_by(|a, b| a.fund_tx_id().as_bytes().cmp(b.fund_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_payment_channel_claims(&self, max: usize) -> Vec<DrcPaymentChannelClaimTx> {
        let mut txs: Vec<_> = self
            .drc_payment_channel_claim_txs
            .values()
            .cloned()
            .collect();
        txs.sort_by(|a, b| a.claim_tx_id().as_bytes().cmp(b.claim_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub fn select_drc_payment_channel_closes(&self, max: usize) -> Vec<DrcPaymentChannelCloseTx> {
        let mut txs: Vec<_> = self
            .drc_payment_channel_close_txs
            .values()
            .cloned()
            .collect();
        txs.sort_by(|a, b| a.close_tx_id().as_bytes().cmp(b.close_tx_id().as_bytes()));
        txs.truncate(max);
        txs
    }

    pub(super) fn payment_channel_maps_len(&self) -> usize {
        self.drc_payment_channel_create_txs.len()
            + self.drc_payment_channel_fund_txs.len()
            + self.drc_payment_channel_claim_txs.len()
            + self.drc_payment_channel_close_txs.len()
    }

    pub(super) fn payment_channel_maps_contains(&self, tx_id: &Hash) -> bool {
        self.drc_payment_channel_create_txs.contains_key(tx_id)
            || self.drc_payment_channel_fund_txs.contains_key(tx_id)
            || self.drc_payment_channel_claim_txs.contains_key(tx_id)
            || self.drc_payment_channel_close_txs.contains_key(tx_id)
    }

    pub(super) fn evict_payment_channel_lanes_from_block(&mut self, block: &agora_types::Block) {
        use agora_types::NativeAssetId;
        for tx in &block.drc_payment_channel_creates {
            let id = tx.channel_id();
            let _ = self.remove_drc_payment_channel_create(&id);
        }
        for tx in &block.drc_payment_channel_funds {
            let id = tx.fund_tx_id();
            let _ = self.remove_drc_payment_channel_fund(&id);
        }
        for tx in &block.drc_payment_channel_claims {
            let id = tx.claim_tx_id();
            let _ = self.remove_drc_payment_channel_claim(&id);
        }
        for tx in &block.drc_payment_channel_closes {
            let id = tx.close_tx_id();
            let _ = self.remove_drc_payment_channel_close(&id);
        }
        let _ = NativeAssetId::DRC;
    }
}
