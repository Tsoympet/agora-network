use agora_types::{
    AccountTransfer, Block, BlockHeader, CheckpointAttestation, DrcAccountPolicyTx,
    DrcCheckCancelTx, DrcCheckCashTx, DrcCheckCreateTx, DrcDepositPreauthTx, DrcEscrowCancelTx,
    DrcEscrowCreateTx, DrcEscrowFinishTx, DrcPaymentTx, DrcRegularKeyTx, DrcSignerListTx,
    DrcTicketCreateTx, Hash, OvlExecutionTx, SignedStakeTx, Transaction,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{ibd::short_ids_for_block, typed_compact::TypedCompactBody};

/// Wire envelopes for gossip payloads.
#[allow(clippy::large_enum_variant)]
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
    /// Appended in Trident protocol v8; owner-authorized DRC recipient policy.
    DrcAccountPolicy(DrcAccountPolicyTx),
    /// Appended in Trident protocol v9; address-based DRC deposit preauthorization.
    DrcDepositPreauth(DrcDepositPreauthTx),
    /// Appended in Trident protocol v11; DRC regular-key rotation.
    DrcRegularKey(DrcRegularKeyTx),
    /// Appended in Trident protocol v12; DRC weighted signer lists.
    DrcSignerList(DrcSignerListTx),
    /// Appended in Trident protocol v15; DRC ticket creation.
    DrcTicketCreate(DrcTicketCreateTx),
    /// Appended in Trident protocol v17; native DRC escrow create.
    DrcEscrowCreate(DrcEscrowCreateTx),
    /// Appended in Trident protocol v17; native DRC escrow finish.
    DrcEscrowFinish(DrcEscrowFinishTx),
    /// Appended in Trident protocol v17; native DRC escrow cancel.
    DrcEscrowCancel(DrcEscrowCancelTx),
    /// Appended in Trident protocol v18; native DRC check create.
    DrcCheckCreate(DrcCheckCreateTx),
    /// Appended in Trident protocol v18; native DRC check cash.
    DrcCheckCash(DrcCheckCashTx),
    /// Appended in Trident protocol v18; native DRC check cancel.
    DrcCheckCancel(DrcCheckCancelTx),
    /// Appended in Trident protocol v19; native DRC payment channel create.
    DrcPaymentChannelCreate(agora_types::DrcPaymentChannelCreateTx),
    /// Appended in Trident protocol v19; native DRC payment channel fund.
    DrcPaymentChannelFund(agora_types::DrcPaymentChannelFundTx),
    /// Appended in Trident protocol v19; native DRC payment channel claim.
    DrcPaymentChannelClaim(agora_types::DrcPaymentChannelClaimTx),
    /// Appended in Trident protocol v19; native DRC payment channel close.
    DrcPaymentChannelClose(agora_types::DrcPaymentChannelCloseTx),
    /// Appended in Trident protocol v20; issuer-scoped trust line set.
    DrcTrustLineSet(agora_types::DrcTrustLineSetTx),
    /// Appended in Trident protocol v20; exact issued-value transfer.
    DrcIssuedTransfer(agora_types::DrcIssuedTransferTx),
    /// Appended in Trident protocol v21; issued-asset policy set.
    DrcIssuedAssetPolicySet(agora_types::DrcIssuedAssetPolicySetTx),
    /// Appended in Trident protocol v21; trust line issuer control.
    DrcTrustLineIssuerControl(agora_types::DrcTrustLineIssuerControlTx),
    /// Appended in Trident protocol v21; issued clawback.
    DrcIssuedClawback(agora_types::DrcIssuedClawbackTx),
    /// Appended in Trident protocol v24; native order-book offer create.
    DrcOfferCreate(agora_types::DrcOfferCreateTx),
    /// Appended in Trident protocol v24; native order-book offer cancel.
    DrcOfferCancel(agora_types::DrcOfferCancelTx),
    /// Appended after offer gossip; TLT covenant spends on the transaction topic.
    TltCovenant(agora_types::TltCovenantTx),
    /// Appended in Trident protocol v25; signed DA authorization on the tx topic.
    DataCommitment(agora_types::DataCommitmentAuthorization),
    /// Appended in Trident protocol v26; raw Ethereum bytes (version 2). Not Agora-signed.
    OvlRawExecution(agora_types::OvlExecutionTx),
    /// Appended in Trident protocol v27; named-lane compact body (not UTXO short ids).
    TypedCompactBlock(TypedCompactBody),
    /// Appended in Trident protocol v28; detached DRC multisign attachments on the tx topic.
    DrcMultisignAttachment(agora_types::DrcMultisignBlockAttachment),
    /// Appended in Trident protocol v29; signed Hub-coordinator passport attestation.
    PassportAttestation(agora_types::PassportAttestation),
    /// Appended in Trident protocol v30; first-coordinator-signed hub registration.
    HubRegistration(agora_types::HubRegistration),
    /// Appended in Trident protocol v30; hub-coordinator-signed grant registration.
    GrantRegistration(agora_types::GrantRegistration),
    /// Appended in Trident protocol v30; hub-coordinator-signed mission registration.
    MissionRegistration(agora_types::MissionRegistration),
    /// Appended in Trident protocol v31; controller-signed protocol treasury spend.
    TreasuryDisbursement(agora_types::TreasuryDisbursement),
    /// Appended in Trident protocol v32; beneficiary-signed vesting unlock.
    VestingUnlock(agora_types::VestingUnlock),
}

impl NetworkMessage {
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("network message borsh encode")
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, borsh::io::Error> {
        borsh::from_slice(bytes)
    }

    /// Build compact gossip.
    ///
    /// UTXO-only blocks keep `CompactBlock`. Named typed lanes, including
    /// detached DRC multisign attachments, use `TypedCompactBlock`. A miss
    /// or unknown kind still falls back to GetBlock.
    pub fn compact_from_block(block: &Block) -> Self {
        if !block.requires_full_body_gossip() {
            Self::CompactBlock {
                header: block.header.clone(),
                short_ids: short_ids_for_block(block),
            }
        } else if let Some(body) = TypedCompactBody::from_block(block) {
            Self::TypedCompactBlock(body)
        } else {
            Self::Block(block.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_compact::TYPED_COMPACT_VERSION;
    use agora_types::{
        Address, Amount, DataAvailabilityCommitment, DataCommitmentAuthorization,
        DrcAccountPolicyTx, Hash,
    };

    fn assert_typed_compact(block: &Block) {
        match NetworkMessage::compact_from_block(block) {
            NetworkMessage::TypedCompactBlock(body) => {
                assert_eq!(body.version, TYPED_COMPACT_VERSION);
                let msg = NetworkMessage::TypedCompactBlock(body);
                assert_eq!(NetworkMessage::decode(&msg.encode()).unwrap(), msg);
                assert_eq!(msg.encode()[0], 35);
            }
            other => panic!("expected typed compact, got {other:?}"),
        }
    }

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

        assert_typed_compact(&block);
    }

    #[test]
    fn data_commitment_block_uses_existing_full_block_variant() {
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
            .data_commitments
            .push(DataCommitmentAuthorization::unsigned(
                Address([7; 20]),
                0,
                DataAvailabilityCommitment::agora_layers_ovolos_batch(
                    "agora-ovolos-testnet-1".into(),
                    Hash([1; 32]),
                    Hash([2; 32]),
                    3,
                    Hash([4; 32]),
                    Hash([5; 32]),
                    Hash([6; 32]),
                    7,
                    8,
                ),
            ));
        block.header.tx_root = block.compute_body_root();

        let message = NetworkMessage::compact_from_block(&block);
        assert_typed_compact(&block);
        assert_eq!(NetworkMessage::decode(&message.encode()).unwrap(), message);
    }

    #[test]
    fn existing_wire_enum_discriminants_are_unchanged() {
        let payment = DrcPaymentTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            agora_types::Amount::from_base_units(1),
            agora_types::Amount::ZERO,
            0,
            Hash([3; 32]),
            0,
        );
        let source_tagged = DrcPaymentTx::unsigned_v2(
            Address([1; 20]),
            Address([2; 20]),
            agora_types::Amount::from_base_units(1),
            agora_types::Amount::ZERO,
            0,
            Some(42),
            Hash([3; 32]),
            0,
        );
        assert_eq!(
            NetworkMessage::Transaction(Transaction::unsigned(1, vec![], vec![], 0)).encode()[0],
            0
        );
        assert_eq!(NetworkMessage::DrcPayment(payment).encode()[0], 9);
        let message = NetworkMessage::DrcPayment(source_tagged);
        assert_eq!(message.encode()[0], 9);
        assert_eq!(NetworkMessage::decode(&message.encode()).unwrap(), message);

        let policy = DrcAccountPolicyTx::set_require_destination_tag(
            Address([4; 20]),
            Amount::from_base_units(1),
            0,
        );
        let message = NetworkMessage::DrcAccountPolicy(policy);
        assert_eq!(message.encode()[0], 10);
        assert_eq!(NetworkMessage::decode(&message.encode()).unwrap(), message);

        let preauth = DrcDepositPreauthTx::authorize(
            Address([4; 20]),
            Address([5; 20]),
            Amount::from_base_units(1),
            0,
        );
        let message = NetworkMessage::DrcDepositPreauth(preauth);
        assert_eq!(message.encode()[0], 11);
        assert_eq!(NetworkMessage::decode(&message.encode()).unwrap(), message);

        let ticket_create =
            DrcTicketCreateTx::unsigned(Address([6; 20]), Amount::from_base_units(1), 0);
        let message = NetworkMessage::DrcTicketCreate(ticket_create.clone());
        assert_eq!(NetworkMessage::decode(&message.encode()).unwrap(), message);
    }

    #[test]
    fn trust_line_lane_block_uses_full_body_gossip() {
        use agora_types::{
            Address, Amount, DrcTrustLineSetTx, Hash, IssuedAmount, IssuedCurrencyCode,
            DRC_TRUST_LINE_SET_TX_VERSION,
        };

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
        block.drc_trust_line_sets.push(DrcTrustLineSetTx {
            version: DRC_TRUST_LINE_SET_TX_VERSION,
            holder: Address([1; 20]),
            issuer: Address([2; 20]),
            currency: IssuedCurrencyCode([0u8; 20]),
            limit: IssuedAmount::from_units(1),
            fee: Amount::from_base_units(1),
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        });
        block.header.tx_root = block.compute_body_root();
        assert_typed_compact(&block);
    }

    #[test]
    fn drc_ticket_create_gossip_roundtrip_and_full_block_lane() {
        let owner = Address([0x33; 20]);
        let create = DrcTicketCreateTx::unsigned(owner, Amount::from_base_units(2), 3);
        let gossip = NetworkMessage::DrcTicketCreate(create.clone());
        assert_eq!(NetworkMessage::decode(&gossip.encode()).unwrap(), gossip);

        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_ticket_creates.push(create);
        block.header.tx_root = block.compute_body_root();
        assert_typed_compact(&block);
    }

    #[test]
    fn tlt_covenant_gossip_is_appended_and_forces_full_block() {
        let tx = agora_types::TltCovenantTx {
            version: agora_types::TLT_COVENANT_TX_VERSION,
            inputs: vec![agora_types::TltCovenantInput {
                previous_outpoint: agora_types::OutPoint {
                    tx_id: Hash([9; 32]),
                    index: 0,
                },
                sequence: agora_types::TLT_SEQUENCE_FINAL,
                script_sig: vec![1],
            }],
            outputs: vec![agora_types::TltCovenantOutput {
                value: agora_types::Amount::from_base_units(1),
                script_pubkey: vec![2],
            }],
            lock_time: 0,
            nonce: 1,
        };
        let gossip = NetworkMessage::TltCovenant(tx.clone());
        assert_eq!(gossip.encode()[0], 32);
        assert_eq!(NetworkMessage::decode(&gossip.encode()).unwrap(), gossip);

        let da = NetworkMessage::DataCommitment(DataCommitmentAuthorization::unsigned(
            Address([7; 20]),
            0,
            DataAvailabilityCommitment::agora_layers_ovolos_batch(
                "agora-ovolos-testnet-1".into(),
                Hash([1; 32]),
                Hash([2; 32]),
                3,
                Hash([4; 32]),
                Hash([5; 32]),
                Hash([6; 32]),
                7,
                8,
            ),
        ));
        assert_eq!(da.encode()[0], 33);
        assert_eq!(NetworkMessage::decode(&da.encode()).unwrap(), da);

        let raw = NetworkMessage::OvlRawExecution(agora_types::OvlExecutionTx::raw_ethereum(vec![
            0x02, 0xc0,
        ]));
        assert_eq!(raw.encode()[0], 34);
        assert_eq!(NetworkMessage::decode(&raw.encode()).unwrap(), raw);

        let compact = NetworkMessage::TypedCompactBlock(crate::typed_compact::TypedCompactBody {
            version: TYPED_COMPACT_VERSION,
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            lanes: vec![],
        });
        assert_eq!(compact.encode()[0], 35);

        let attachment =
            NetworkMessage::DrcMultisignAttachment(agora_types::DrcMultisignBlockAttachment {
                version: 1,
                key: agora_types::DrcMultisignAttachmentKey {
                    version: 1,
                    kind: agora_types::DrcMultisignOperationKind::DrcPayment,
                    signing_commitment: Hash([9; 32]),
                },
                auth: agora_types::DrcMultisignAuth {
                    version: 1,
                    signing_for: Address([9; 20]),
                    signatures: vec![agora_types::DrcMultisignEntry {
                        signer: Address([10; 20]),
                        public_key: vec![2; 33],
                        signature: vec![3; 64],
                    }],
                },
            });
        assert_eq!(attachment.encode()[0], 36);
        assert_eq!(
            NetworkMessage::decode(&attachment.encode()).unwrap(),
            attachment
        );

        let passport = NetworkMessage::PassportAttestation(agora_types::PassportAttestation {
            version: 1,
            issuer: Address([1; 20]),
            subject: Address([2; 20]),
            category: agora_types::PassportCategory::Code,
            evidence_hash: Hash([3; 32]),
            issuer_policy_hash: Hash([4; 32]),
            issued_epoch: 5,
            expires_epoch: Some(10),
            nonce: 0,
            public_key: vec![1; 33],
            signature: vec![2; 64],
        });
        assert_eq!(passport.encode()[0], 37);
        assert_eq!(
            NetworkMessage::decode(&passport.encode()).unwrap(),
            passport
        );

        let hub = NetworkMessage::HubRegistration(agora_types::HubRegistration::unsigned(
            "Agora Hub".into(),
            "Geographic".into(),
            Hash([2; 32]),
            vec![Address([3; 20])],
            Address([4; 20]),
            12,
            3,
            Hash([5; 32]),
            Hash([6; 32]),
            1,
            0,
        ));
        assert_eq!(hub.encode()[0], 38);
        assert_eq!(NetworkMessage::decode(&hub.encode()).unwrap(), hub);

        let grant = NetworkMessage::GrantRegistration(agora_types::GrantRegistration::unsigned(
            Address([1; 20]),
            7,
            agora_types::TreasuryId::OvlBuilder,
            Address([2; 20]),
            agora_types::Amount::from_base_units(10),
            agora_types::CommunityGrantKind::Micro,
            vec![],
            Hash::ZERO,
            0,
        ));
        assert_eq!(grant.encode()[0], 39);
        assert_eq!(NetworkMessage::decode(&grant.encode()).unwrap(), grant);

        let mission =
            NetworkMessage::MissionRegistration(agora_types::MissionRegistration::unsigned(
                Address([1; 20]),
                agora_types::TreasuryId::DrcCommunity,
                agora_types::Amount::from_base_units(5),
                Hash([3; 32]),
                0,
            ));
        assert_eq!(mission.encode()[0], 40);
        assert_eq!(NetworkMessage::decode(&mission.encode()).unwrap(), mission);

        let treasury =
            NetworkMessage::TreasuryDisbursement(agora_types::TreasuryDisbursement::unsigned(
                agora_types::TreasuryId::OvlBuilder,
                Address([1; 20]),
                agora_types::Amount::from_base_units(10),
                Hash([2; 32]),
                Hash([3; 32]),
                0,
            ));
        assert_eq!(treasury.encode()[0], 41);
        assert_eq!(
            NetworkMessage::decode(&treasury.encode()).unwrap(),
            treasury
        );

        let vesting = NetworkMessage::VestingUnlock(agora_types::VestingUnlock::unsigned(
            agora_types::NativeAssetId::OVL,
            Address([1; 20]),
            Hash([2; 32]),
            agora_types::Amount::from_base_units(10),
            0,
        ));
        assert_eq!(vesting.encode()[0], 42);
        assert_eq!(NetworkMessage::decode(&vesting.encode()).unwrap(), vesting);

        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.tlt_covenants.push(tx);
        block.header.tx_root = block.compute_body_root();
        assert_typed_compact(&block);
    }

    #[test]
    fn drc_offer_lane_forces_full_block() {
        use agora_types::{
            Address, Amount, DrcBookAsset, DrcOfferCreateTx, IssuedAssetId, IssuedCurrencyCode,
            DRC_OFFER_CREATE_TX_VERSION,
        };

        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.drc_offer_creates.push(DrcOfferCreateTx {
            version: DRC_OFFER_CREATE_TX_VERSION,
            owner: Address([1; 20]),
            taker_pays: DrcBookAsset::NativeDrc,
            taker_pays_amount: 2,
            taker_gets: DrcBookAsset::Issued(IssuedAssetId {
                issuer: Address([2; 20]),
                currency: IssuedCurrencyCode([3u8; 20]),
            }),
            taker_gets_amount: 1,
            fill_mode: 0,
            time_in_force: 0,
            fee: Amount::from_base_units(1),
            expires_after_blue_score: None,
            nonce: 0,
            account_sequence: None,
            public_key: vec![],
            signature: vec![],
            multisign: None,
        });
        block.header.tx_root = block.compute_body_root();
        assert_typed_compact(&block);
        let empty = Block::utxo(
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
        assert!(!empty.requires_full_body_gossip());
    }
}
