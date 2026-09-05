use agora_types::{
    AccountTransfer, Block, BlockHeader, CheckpointAttestation, DrcPaymentTx, Hash, OvlExecutionTx,
    SignedStakeTx, Transaction,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::ibd::short_ids_for_block;

/// Wire envelopes for gossip payloads.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NetworkMessage {
    Transaction(Transaction),
    Block(Block),
    /// Hash-only tip signal; peers that lack the body issue [`Self::GetBlock`].
    BlockAnnounce {
        hash: Hash,
    },
    /// Header + short tx ids for mempool inflation (BIP152-style scaffold).
    CompactBlock {
        header: BlockHeader,
        short_ids: Vec<[u8; 8]>,
    },
    /// IBD / compact-miss follow-up: request the full block body by hash.
    GetBlock {
        hash: Hash,
    },
    /// Trident dual-PoS checkpoint attestation (OVL or DRC validator).
    CheckpointAttestation(CheckpointAttestation),
    /// Appended to preserve all pre-v2 Borsh enum discriminants.
    AccountTransfer(AccountTransfer),
    /// Appended to preserve all pre-v2 Borsh enum discriminants.
    StakeTx(SignedStakeTx),
    /// Appended in Trident protocol v3; signed OVL execution envelope.
    OvlExecution(OvlExecutionTx),
    /// Appended in Trident protocol v4; native DRC payment envelope.
    DrcPayment(DrcPaymentTx),
}

impl NetworkMessage {
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("network message borsh encode")
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, borsh::io::Error> {
        borsh::from_slice(bytes)
    }

    /// Build compact gossip for UTXO-only blocks.
    ///
    /// Multi-lane blocks use the full body until a versioned compact format can
    /// commit lane kinds without ambiguity.
    pub fn compact_from_block(block: &Block) -> Self {
        if block.account_transfers.is_empty()
            && block.stake_ops.is_empty()
            && block.ovl_executions.is_empty()
            && block.drc_payments.is_empty()
        {
            Self::CompactBlock {
                header: block.header.clone(),
                short_ids: short_ids_for_block(block),
            }
        } else {
            Self::Block(block.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::Hash;

    #[test]
    fn compact_and_get_block_roundtrip() {
        let header = BlockHeader {
            version: 1,
            parents: vec![Hash::ZERO],
            timestamp_ms: 9,
            bits: 1,
            nonce: 2,
            tx_root: Hash::ZERO,
        };
        let compact = NetworkMessage::CompactBlock {
            header: header.clone(),
            short_ids: vec![],
        };
        let decoded = NetworkMessage::decode(&compact.encode()).unwrap();
        assert_eq!(decoded, compact);

        let get = NetworkMessage::GetBlock {
            hash: header.hash(),
        };
        assert_eq!(NetworkMessage::decode(&get.encode()).unwrap(), get);

        let att = NetworkMessage::CheckpointAttestation(CheckpointAttestation {
            body: agora_types::CheckpointBody {
                chain_id: "c".into(),
                genesis_hash: Hash::ZERO,
                consensus_policy_hash: Hash::ZERO,
                state_transition_version: "v".into(),
                blue_score: 1,
                block_hash: Hash([1u8; 32]),
                state_root: Hash::ZERO,
                validator_epoch: 0,
            },
            set: agora_types::NativeAssetId::OVL,
            validator: agora_types::Address([2u8; 20]),
            public_key: vec![0; 33],
            signature: vec![0; 64],
        });
        assert_eq!(NetworkMessage::decode(&att.encode()).unwrap(), att);
    }

    #[test]
    fn multi_lane_block_uses_full_body_gossip() {
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block
            .account_transfers
            .push(AccountTransfer::unsigned_with_fee(
                agora_types::NativeAssetId::OVL,
                agora_types::Address::ZERO,
                agora_types::Address([1; 20]),
                agora_types::Amount::from_base_units(2),
                agora_types::Amount::from_base_units(1),
                0,
            ));
        block.header.tx_root = block.compute_body_root();

        let message = NetworkMessage::compact_from_block(&block);
        assert_eq!(message, NetworkMessage::Block(block));
    }
}
