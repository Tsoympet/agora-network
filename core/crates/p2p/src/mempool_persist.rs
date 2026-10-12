//! Portable mempool snapshot for `--no-default-features` datadirs.
//!
//! Envelopes are written to `$AGORA_DATA/portable/mempool.bin`. Reservations
//! are rebuilt by re-admitting. This is not RocksDB and not a public-testnet
//! durability claim.

use std::path::{Path, PathBuf};

use agora_types::{
    AccountTransfer, DataCommitmentAuthorization, DrcAccountPolicyTx, DrcCheckCancelTx,
    DrcCheckCashTx, DrcCheckCreateTx, DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx,
    DrcEscrowFinishTx, DrcIssuedAssetPolicySetTx, DrcIssuedClawbackTx, DrcIssuedTransferTx,
    DrcMultisignBlockAttachment, DrcOfferCancelTx, DrcOfferCreateTx, DrcPaymentChannelClaimTx,
    DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx, DrcPaymentChannelFundTx, DrcPaymentTx,
    DrcRegularKeyTx, DrcSignerListTx, DrcTicketCreateTx, DrcTrustLineIssuerControlTx,
    DrcTrustLineSetTx, GrantRegistration, HubRegistration, MissionRegistration, OvlExecutionTx,
    PassportAttestation, SignedStakeTx, TltCovenantTx, Transaction, TreasuryDisbursement,
    VestingUnlock,
};
use borsh::{BorshDeserialize, BorshSerialize};
use tracing::warn;

use super::Mempool;
use crate::P2pError;

const PORTABLE_MAGIC: &[u8; 8] = b"AGORAMP1";

/// `$data_dir/portable/mempool.bin` — sibling of the portable state snapshot.
pub fn mempool_persist_path(data_dir: impl AsRef<Path>) -> PathBuf {
    data_dir.as_ref().join("portable").join("mempool.bin")
}

#[derive(BorshSerialize, BorshDeserialize)]
struct DurableTlt {
    tx: Transaction,
    fee: u64,
    rbf: bool,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct DurableCovenant {
    tx: TltCovenantTx,
    fee: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct DurablePayment {
    tx: DrcPaymentTx,
    deposit_auth: bool,
    preauthorized: bool,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct DurableTrustLine {
    tx: DrcTrustLineSetTx,
    pending_create: bool,
    pending_delete: bool,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct DurableMempoolV1 {
    max_size: u64,
    tlt: Vec<DurableTlt>,
    covenants: Vec<DurableCovenant>,
    accounts: Vec<AccountTransfer>,
    stakes: Vec<SignedStakeTx>,
    ovl: Vec<OvlExecutionTx>,
    ovl_raw: Vec<OvlExecutionTx>,
    payments: Vec<DurablePayment>,
    policies: Vec<DrcAccountPolicyTx>,
    preauths: Vec<DrcDepositPreauthTx>,
    regular_keys: Vec<DrcRegularKeyTx>,
    signer_lists: Vec<DrcSignerListTx>,
    tickets: Vec<DrcTicketCreateTx>,
    escrow_creates: Vec<DrcEscrowCreateTx>,
    escrow_finishes: Vec<DrcEscrowFinishTx>,
    escrow_cancels: Vec<DrcEscrowCancelTx>,
    check_creates: Vec<DrcCheckCreateTx>,
    check_cashes: Vec<DrcCheckCashTx>,
    check_cancels: Vec<DrcCheckCancelTx>,
    channel_creates: Vec<DrcPaymentChannelCreateTx>,
    channel_funds: Vec<DrcPaymentChannelFundTx>,
    channel_claims: Vec<DrcPaymentChannelClaimTx>,
    channel_closes: Vec<DrcPaymentChannelCloseTx>,
    trust_lines: Vec<DurableTrustLine>,
    issued_transfers: Vec<DrcIssuedTransferTx>,
    asset_policies: Vec<DrcIssuedAssetPolicySetTx>,
    issuer_controls: Vec<DrcTrustLineIssuerControlTx>,
    clawbacks: Vec<DrcIssuedClawbackTx>,
    offer_creates: Vec<DrcOfferCreateTx>,
    offer_cancels: Vec<DrcOfferCancelTx>,
    data_commitments: Vec<DataCommitmentAuthorization>,
    multisign: Vec<DrcMultisignBlockAttachment>,
    passports: Vec<PassportAttestation>,
    hubs: Vec<HubRegistration>,
    grants: Vec<GrantRegistration>,
    missions: Vec<MissionRegistration>,
    treasuries: Vec<TreasuryDisbursement>,
    vestings: Vec<VestingUnlock>,
}

impl Mempool {
    /// Open a portable snapshot, or an empty pool that will persist there.
    pub fn open_persistent(path: impl AsRef<Path>, max_size: usize) -> Result<Self, P2pError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            let mut pool = Self::new(max_size);
            pool.persist_path = Some(path);
            return Ok(pool);
        }
        let snap = load_snapshot(&path)?;
        let mut pool = restore_snapshot(snap, max_size);
        pool.persist_path = Some(path);
        Ok(pool)
    }

    pub fn set_persist_path(&mut self, path: impl AsRef<Path>) {
        self.persist_path = Some(path.as_ref().to_path_buf());
    }

    pub(crate) fn persist_best_effort(&self) {
        let Some(path) = self.persist_path.as_ref() else {
            return;
        };
        if let Err(err) = write_snapshot(path, &self.snapshot()) {
            warn!(error = %err, path = %path.display(), "durable mempool flush failed");
        }
    }

    fn snapshot(&self) -> DurableMempoolV1 {
        DurableMempoolV1 {
            max_size: self.max_size as u64,
            tlt: self
                .txs
                .iter()
                .map(|(id, tx)| DurableTlt {
                    tx: tx.clone(),
                    fee: self.fees.get(id).copied().unwrap_or(0),
                    rbf: self.rbf_opt_in.contains(id),
                })
                .collect(),
            covenants: self
                .covenant_txs
                .iter()
                .map(|(id, tx)| DurableCovenant {
                    tx: tx.clone(),
                    fee: self.fees.get(id).copied().unwrap_or(0),
                })
                .collect(),
            accounts: self.account_txs.values().cloned().collect(),
            stakes: self.stake_txs.values().cloned().collect(),
            ovl: self.ovl_execution_txs.values().cloned().collect(),
            ovl_raw: self.ovl_raw_execution_txs.values().cloned().collect(),
            payments: self
                .payment_txs
                .iter()
                .map(|(id, tx)| DurablePayment {
                    tx: tx.clone(),
                    deposit_auth: self.deposit_auth_required_payments.contains(id),
                    preauthorized: self.deposit_preauthorized_payments.contains(id),
                })
                .collect(),
            policies: self.drc_policy_txs.values().cloned().collect(),
            preauths: self.drc_deposit_preauth_txs.values().cloned().collect(),
            regular_keys: self.drc_regular_key_txs.values().cloned().collect(),
            signer_lists: self.drc_signer_list_txs.values().cloned().collect(),
            tickets: self.drc_ticket_create_txs.values().cloned().collect(),
            escrow_creates: self.drc_escrow_create_txs.values().cloned().collect(),
            escrow_finishes: self.drc_escrow_finish_txs.values().cloned().collect(),
            escrow_cancels: self.drc_escrow_cancel_txs.values().cloned().collect(),
            check_creates: self.drc_check_create_txs.values().cloned().collect(),
            check_cashes: self.drc_check_cash_txs.values().cloned().collect(),
            check_cancels: self.drc_check_cancel_txs.values().cloned().collect(),
            channel_creates: self
                .drc_payment_channel_create_txs
                .values()
                .cloned()
                .collect(),
            channel_funds: self
                .drc_payment_channel_fund_txs
                .values()
                .cloned()
                .collect(),
            channel_claims: self
                .drc_payment_channel_claim_txs
                .values()
                .cloned()
                .collect(),
            channel_closes: self
                .drc_payment_channel_close_txs
                .values()
                .cloned()
                .collect(),
            trust_lines: self
                .drc_trust_line_set_txs
                .values()
                .map(|tx| {
                    let slot = (tx.holder, tx.asset_id().asset_key());
                    DurableTrustLine {
                        tx: tx.clone(),
                        pending_create: self.pending_trust_line_create_slots.contains(&slot),
                        pending_delete: self.pending_trust_line_delete_slots.contains(&slot),
                    }
                })
                .collect(),
            issued_transfers: self.drc_issued_transfer_txs.values().cloned().collect(),
            asset_policies: self
                .drc_issued_asset_policy_set_txs
                .values()
                .cloned()
                .collect(),
            issuer_controls: self
                .drc_trust_line_issuer_control_txs
                .values()
                .cloned()
                .collect(),
            clawbacks: self.drc_issued_clawback_txs.values().cloned().collect(),
            offer_creates: self.drc_offer_create_txs.values().cloned().collect(),
            offer_cancels: self.drc_offer_cancel_txs.values().cloned().collect(),
            data_commitments: self.data_commitments.values().cloned().collect(),
            multisign: self.drc_multisign_attachments.values().cloned().collect(),
            passports: self.passport_attestations.values().cloned().collect(),
            hubs: self.hub_registrations.values().cloned().collect(),
            grants: self.grant_registrations.values().cloned().collect(),
            missions: self.mission_registrations.values().cloned().collect(),
            treasuries: self.treasury_disbursements.values().cloned().collect(),
            vestings: self.vesting_unlocks.values().cloned().collect(),
        }
    }
}

fn restore_snapshot(snap: DurableMempoolV1, fallback_max: usize) -> Mempool {
    let max = if snap.max_size == 0 {
        fallback_max
    } else {
        snap.max_size as usize
    };
    let mut pool = Mempool::new(max);
    for item in snap.tlt {
        let _ = if item.rbf {
            pool.admit_priced_opt_in(item.tx, item.fee, true)
        } else {
            pool.admit_priced(item.tx, item.fee)
        };
    }
    for item in snap.covenants {
        let _ = pool.admit_covenant(item.tx, item.fee);
    }
    for tx in snap.accounts {
        let _ = pool.admit_account(tx);
    }
    for tx in snap.stakes {
        let _ = pool.admit_stake(tx);
    }
    for tx in snap.ovl {
        let _ = pool.admit_ovl_execution(tx);
    }
    for tx in snap.ovl_raw {
        let _ = pool.admit_ovl_raw_execution(tx);
    }
    for item in snap.payments {
        let _ = if item.deposit_auth || item.preauthorized {
            pool.admit_payment_with_deposit_auth_at_blue_score(
                item.tx,
                0,
                item.deposit_auth,
                item.preauthorized,
            )
        } else if item.tx.version == agora_types::DRC_PAYMENT_VERSION {
            pool.admit_payment_at_blue_score(item.tx, 0)
        } else {
            pool.admit_payment(item.tx)
        };
    }
    for tx in snap.policies {
        let _ = pool.admit_drc_policy(tx);
    }
    for tx in snap.preauths {
        let _ = pool.admit_drc_deposit_preauth(tx);
    }
    for tx in snap.regular_keys {
        let _ = pool.admit_drc_regular_key(tx);
    }
    for tx in snap.signer_lists {
        let _ = pool.admit_drc_signer_list(tx);
    }
    for tx in snap.tickets {
        let _ = pool.admit_drc_ticket_create(tx);
    }
    for tx in snap.escrow_creates {
        let _ = pool.admit_drc_escrow_create(tx);
    }
    for tx in snap.escrow_finishes {
        let _ = pool.admit_drc_escrow_finish(tx);
    }
    for tx in snap.escrow_cancels {
        let _ = pool.admit_drc_escrow_cancel(tx);
    }
    for tx in snap.check_creates {
        let _ = pool.admit_drc_check_create(tx);
    }
    for tx in snap.check_cashes {
        let _ = pool.admit_drc_check_cash(tx);
    }
    for tx in snap.check_cancels {
        let _ = pool.admit_drc_check_cancel(tx);
    }
    for tx in snap.channel_creates {
        let _ = pool.admit_drc_payment_channel_create(tx);
    }
    for tx in snap.channel_funds {
        let _ = pool.admit_drc_payment_channel_fund(tx);
    }
    for tx in snap.channel_claims {
        let _ = pool.admit_drc_payment_channel_claim(tx);
    }
    for tx in snap.channel_closes {
        let _ = pool.admit_drc_payment_channel_close(tx);
    }
    for item in snap.trust_lines {
        let _ = pool.admit_drc_trust_line_set(item.tx, item.pending_create, item.pending_delete);
    }
    for tx in snap.issued_transfers {
        let _ = pool.admit_drc_issued_transfer(tx);
    }
    for tx in snap.asset_policies {
        let _ = pool.admit_drc_issued_asset_policy_set(tx, Vec::new());
    }
    for tx in snap.issuer_controls {
        let _ = pool.admit_drc_trust_line_issuer_control(tx);
    }
    for tx in snap.clawbacks {
        let _ = pool.admit_drc_issued_clawback(tx);
    }
    for tx in snap.offer_creates {
        let _ = pool.admit_drc_offer_create(tx);
    }
    for tx in snap.offer_cancels {
        let _ = pool.admit_drc_offer_cancel(tx);
    }
    for tx in snap.data_commitments {
        let _ = pool.admit_data_commitment(tx);
    }
    for tx in snap.multisign {
        let _ = pool.admit_drc_multisign_attachment(tx);
    }
    for tx in snap.passports {
        let _ = pool.admit_passport_attestation(tx);
    }
    for tx in snap.hubs {
        let _ = pool.admit_hub_registration(tx);
    }
    for tx in snap.grants {
        let _ = pool.admit_grant_registration(tx);
    }
    for tx in snap.missions {
        let _ = pool.admit_mission_registration(tx);
    }
    for tx in snap.treasuries {
        let _ = pool.admit_treasury_disbursement(tx);
    }
    for tx in snap.vestings {
        let _ = pool.admit_vesting_unlock(tx);
    }
    pool
}

fn write_snapshot(path: &Path, snap: &DurableMempoolV1) -> Result<(), P2pError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| P2pError::Decode(e.to_string()))?;
    }
    let payload = borsh::to_vec(snap).map_err(|e| P2pError::Decode(e.to_string()))?;
    let mut bytes = Vec::with_capacity(PORTABLE_MAGIC.len() + payload.len());
    bytes.extend_from_slice(PORTABLE_MAGIC);
    bytes.extend_from_slice(&payload);
    let tmp = path.with_extension("bin.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| P2pError::Decode(e.to_string()))?;
    if let Ok(file) = std::fs::File::open(&tmp) {
        let _ = file.sync_all();
    }
    std::fs::rename(&tmp, path).map_err(|e| P2pError::Decode(e.to_string()))?;
    Ok(())
}

fn load_snapshot(path: &Path) -> Result<DurableMempoolV1, P2pError> {
    let bytes = std::fs::read(path).map_err(|e| P2pError::Decode(e.to_string()))?;
    if bytes.len() < PORTABLE_MAGIC.len() || !bytes.starts_with(PORTABLE_MAGIC) {
        return Err(P2pError::Decode(
            "portable mempool snapshot is missing AGORAMP1 magic".into(),
        ));
    }
    DurableMempoolV1::try_from_slice(&bytes[PORTABLE_MAGIC.len()..])
        .map_err(|e| P2pError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_crypto::{sign_transaction, KeyPair};
    use agora_types::{Amount, Hash, OutPoint, TxIn, TxOut};

    fn signed_spend(index: u32, nonce: u64) -> Transaction {
        let kp = KeyPair::from_secret_bytes(&[0x42; 32]).unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: kp.address(),
            }],
            nonce,
        );
        sign_transaction(&mut tx, &kp).unwrap();
        tx
    }

    #[test]
    fn tlt_survives_portable_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        let tx = signed_spend(0, 1);
        let id = tx.tx_id();
        {
            let mut pool = Mempool::open_persistent(&path, 16).unwrap();
            pool.admit_priced(tx.clone(), 7).unwrap();
            assert!(path.is_file());
        }
        let pool = Mempool::open_persistent(&path, 16).unwrap();
        assert!(pool.contains(&id));
        assert_eq!(pool.fee_of(&id), Some(7));
        assert_eq!(pool.pending_entries(8)[0].0.tx_id(), id);
        assert!(pool.reserved().contains(&OutPoint {
            tx_id: Hash::ZERO,
            index: 0,
        }));
    }

    #[test]
    fn evict_clears_portable_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        let tx = signed_spend(3, 4);
        let id = tx.tx_id();
        let mut block = agora_types::Block::utxo(
            agora_types::BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 1,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![tx.clone()],
        );
        block.header.tx_root = block.compute_body_root();
        {
            let mut pool = Mempool::open_persistent(&path, 8).unwrap();
            pool.admit_priced(tx, 1).unwrap();
            pool.evict_for_block(&block);
        }
        let pool = Mempool::open_persistent(&path, 8).unwrap();
        assert!(!pool.contains(&id));
        assert!(pool.pending_entries(8).is_empty());
    }

    #[test]
    fn compact_inflate_survives_portable_reopen() {
        use crate::ibd::{short_ids_for_block, ReconstructError};
        use crate::typed_compact::COMPACT_LANE_ACCOUNT;
        use crate::NetworkMessage;
        use agora_types::{AccountTransfer, Address, BlockHeader, NativeAssetId};

        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        let tx = signed_spend(1, 2);
        let account = AccountTransfer::unsigned_with_fee(
            NativeAssetId::OVL,
            Address::ZERO,
            Address([1; 20]),
            Amount::from_base_units(2),
            Amount::from_base_units(1),
            0,
        );
        {
            let mut pool = Mempool::open_persistent(&path, 16).unwrap();
            pool.admit_priced(tx.clone(), 3).unwrap();
            pool.admit_account(account.clone()).unwrap();
        }

        let pool = Mempool::open_persistent(&path, 16).unwrap();

        let mut utxo = agora_types::Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 4,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![tx.clone()],
        );
        utxo.header.tx_root = agora_types::Block::compute_tx_root(&utxo.transactions);
        match NetworkMessage::compact_from_block(&utxo) {
            NetworkMessage::CompactBlock { header, short_ids } => {
                assert_eq!(short_ids, short_ids_for_block(&utxo));
                let rebuilt = pool.inflate_compact_block(header, &short_ids).unwrap();
                assert_eq!(rebuilt.transactions, vec![tx.clone()]);
            }
            other => panic!("expected UTXO CompactBlock, got {other:?}"),
        }

        let mut typed = agora_types::Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 5,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![tx.clone()],
        );
        typed.account_transfers.push(account.clone());
        typed.header.tx_root = typed.compute_body_root();
        match NetworkMessage::compact_from_block(&typed) {
            NetworkMessage::TypedCompactBlock(body) => {
                assert!(body
                    .lanes
                    .iter()
                    .any(|lane| lane.kind == COMPACT_LANE_ACCOUNT));
                let rebuilt = pool.inflate_typed_compact(body).unwrap();
                assert_eq!(rebuilt.transactions, vec![tx]);
                assert_eq!(rebuilt.account_transfers, vec![account]);
            }
            other => panic!("expected TypedCompactBlock, got {other:?}"),
        }

        let empty =
            Mempool::open_persistent(mempool_persist_path(dir.path().join("empty")), 8).unwrap();
        let sid = crate::ibd::tx_short_id(&signed_spend(9, 9).tx_id());
        let err = empty
            .inflate_compact_block(utxo.header.clone(), &[sid])
            .unwrap_err();
        assert!(matches!(err, ReconstructError::MissingShortIds(1)));
    }

    #[test]
    fn truncated_magic_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"NOPE").unwrap();
        match Mempool::open_persistent(&path, 8) {
            Err(P2pError::Decode(msg)) => assert!(msg.contains("AGORAMP1")),
            Err(other) => panic!("unexpected {other}"),
            Ok(_) => panic!("expected truncated snapshot to fail"),
        }
    }
}
