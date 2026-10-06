//! Mempool index for detached DRC multisign attachments.
//!
//! Typed compact names this lane (`COMPACT_LANE_DRC_MULTISIGN`). A miss still
//! falls back to GetBlock.

use agora_types::{DrcMultisignBlockAttachment, Hash};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn get_drc_multisign_attachment(&self, id: &Hash) -> Option<&DrcMultisignBlockAttachment> {
        self.drc_multisign_attachments.get(id)
    }

    pub fn admit_drc_multisign_attachment(
        &mut self,
        attachment: DrcMultisignBlockAttachment,
    ) -> Result<Hash, P2pError> {
        attachment.validate().map_err(|err| {
            P2pError::MempoolRejected(format!("invalid multisign attachment: {err}"))
        })?;
        let id = attachment.body_commitment_id();
        if self.drc_multisign_attachments.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        self.drc_multisign_attachments.insert(id, attachment);
        Ok(id)
    }

    pub fn index_drc_multisign_attachments_from_block(&mut self, block: &agora_types::Block) {
        for attachment in &block.drc_multisign_attachments {
            let _ = self.admit_drc_multisign_attachment(attachment.clone());
        }
    }

    pub(super) fn evict_drc_multisign_attachments_from_block(
        &mut self,
        block: &agora_types::Block,
    ) {
        for attachment in &block.drc_multisign_attachments {
            self.drc_multisign_attachments
                .remove(&attachment.body_commitment_id());
        }
    }
}

#[cfg(test)]
mod tests {
    use agora_types::{
        DrcMultisignAttachmentKey, DrcMultisignAuth, DrcMultisignBlockAttachment,
        DrcMultisignEntry, DrcMultisignOperationKind, Hash,
    };

    use crate::ibd::ReconstructError;
    use crate::typed_compact::{
        reconstruct_typed_compact, TypedCompactBody, COMPACT_LANE_DRC_MULTISIGN,
    };
    use crate::Mempool;

    fn sample_attachment(marker: u8) -> DrcMultisignBlockAttachment {
        DrcMultisignBlockAttachment {
            version: 1,
            key: DrcMultisignAttachmentKey {
                version: 1,
                kind: DrcMultisignOperationKind::DrcPayment,
                signing_commitment: Hash([marker; 32]),
            },
            auth: DrcMultisignAuth {
                version: 1,
                signing_for: agora_types::Address([marker; 20]),
                signatures: vec![DrcMultisignEntry {
                    signer: agora_types::Address([marker.wrapping_add(1); 20]),
                    public_key: vec![2; 33],
                    signature: vec![3; 64],
                }],
            },
        }
    }

    #[test]
    fn admits_and_evicts_multisign_attachments() {
        let attachment = sample_attachment(9);
        let id = attachment.body_commitment_id();
        let mut pool = Mempool::new(8);
        assert_eq!(
            pool.admit_drc_multisign_attachment(attachment.clone())
                .unwrap(),
            id
        );
        assert!(pool.get_drc_multisign_attachment(&id).is_some());
        let mut block = agora_types::Block::utxo(
            agora_types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_multisign_attachments.push(attachment);
        pool.evict_drc_multisign_attachments_from_block(&block);
        assert!(pool.get_drc_multisign_attachment(&id).is_none());
    }

    #[test]
    fn compact_reconstructs_multisign_from_mempool_and_misses_without() {
        let attachment = sample_attachment(9);
        let mut block = agora_types::Block::utxo(
            agora_types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_multisign_attachments.push(attachment.clone());
        block.header.tx_root = block.compute_body_root();

        let body = TypedCompactBody::from_block(&block).expect("named");
        assert_eq!(body.lanes[0].kind, COMPACT_LANE_DRC_MULTISIGN);

        let mut pool = Mempool::new(8);
        assert_eq!(
            pool.admit_drc_multisign_attachment(attachment).unwrap(),
            block.drc_multisign_attachments[0].body_commitment_id()
        );
        let rebuilt = reconstruct_typed_compact(body.clone(), |kind, sid| {
            pool.clone_typed_lane_item(kind, sid)
        })
        .unwrap();
        assert_eq!(
            rebuilt.drc_multisign_attachments,
            block.drc_multisign_attachments
        );

        let empty = Mempool::new(8);
        let err =
            reconstruct_typed_compact(body, |kind, sid| empty.clone_typed_lane_item(kind, sid))
                .unwrap_err();
        assert!(matches!(err, ReconstructError::MissingShortIds(1)));
    }
}
