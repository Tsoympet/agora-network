//! Portable mempool WAL + snapshot for `--no-default-features` datadirs.
//!
//! High-churn admits append framed upserts to `$AGORA_DATA/portable/mempool.wal`
//! (`AGORAW01`). Evicts and every 32 upserts compact into `mempool.bin`
//! (`AGORAMP1`) via tmp+rename. Reservations are rebuilt by re-admitting.
//! This is not RocksDB and not a public-testnet durability claim.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
use agora_types::Hash;

const PORTABLE_MAGIC: &[u8; 8] = b"AGORAMP1";
const WAL_MAGIC: &[u8; 8] = b"AGORAW01";
const CHECKPOINT_EVERY: u32 = 32;

/// `$data_dir/portable/mempool.bin` — sibling of the portable state snapshot.
pub fn mempool_persist_path(data_dir: impl AsRef<Path>) -> PathBuf {
    data_dir.as_ref().join("portable").join("mempool.bin")
}

/// `$data_dir/portable/mempool.wal` — append-only upserts between checkpoints.
pub fn mempool_wal_path(data_dir: impl AsRef<Path>) -> PathBuf {
    mempool_persist_path(data_dir).with_extension("wal")
}

#[derive(Debug)]
pub(crate) struct MempoolWalMeta {
    file: File,
    ops_since_checkpoint: u32,
    live: HashMap<Hash, u32>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
enum DurableEnvelope {
    Tlt(DurableTlt),
    Covenant(DurableCovenant),
    Account(AccountTransfer),
    Stake(SignedStakeTx),
    Ovl(OvlExecutionTx),
    OvlRaw(OvlExecutionTx),
    Payment(DurablePayment),
    Policy(DrcAccountPolicyTx),
    Preauth(DrcDepositPreauthTx),
    RegularKey(DrcRegularKeyTx),
    SignerList(DrcSignerListTx),
    Ticket(DrcTicketCreateTx),
    EscrowCreate(DrcEscrowCreateTx),
    EscrowFinish(DrcEscrowFinishTx),
    EscrowCancel(DrcEscrowCancelTx),
    CheckCreate(DrcCheckCreateTx),
    CheckCash(DrcCheckCashTx),
    CheckCancel(DrcCheckCancelTx),
    ChannelCreate(DrcPaymentChannelCreateTx),
    ChannelFund(DrcPaymentChannelFundTx),
    ChannelClaim(DrcPaymentChannelClaimTx),
    ChannelClose(DrcPaymentChannelCloseTx),
    TrustLine(DurableTrustLine),
    IssuedTransfer(DrcIssuedTransferTx),
    AssetPolicy(DrcIssuedAssetPolicySetTx),
    IssuerControl(DrcTrustLineIssuerControlTx),
    Clawback(DrcIssuedClawbackTx),
    OfferCreate(DrcOfferCreateTx),
    OfferCancel(DrcOfferCancelTx),
    DataCommitment(DataCommitmentAuthorization),
    Multisign(DrcMultisignBlockAttachment),
    Passport(PassportAttestation),
    Hub(HubRegistration),
    Grant(GrantRegistration),
    Mission(MissionRegistration),
    Treasury(TreasuryDisbursement),
    Vesting(VestingUnlock),
}

#[derive(BorshSerialize, BorshDeserialize)]
enum MempoolWalOp {
    Upsert(DurableEnvelope),
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct DurableTlt {
    tx: Transaction,
    fee: u64,
    rbf: bool,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct DurableCovenant {
    tx: TltCovenantTx,
    fee: u64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct DurablePayment {
    tx: DrcPaymentTx,
    deposit_auth: bool,
    preauthorized: bool,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
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
        let wal_path = path.with_extension("wal");
        let mut pool = if path.exists() {
            restore_snapshot(load_snapshot(&path)?, max_size)
        } else {
            Self::new(max_size)
        };
        let replayed = replay_mempool_wal(&wal_path)?;
        let replayed_len = u32::try_from(replayed.len()).unwrap_or(u32::MAX);
        for op in replayed {
            apply_mempool_wal_op(&mut pool, op);
        }
        let live = fingerprints(&pool.snapshot());
        let mut meta = MempoolWalMeta {
            file: open_mempool_wal(&wal_path)?,
            ops_since_checkpoint: replayed_len,
            live,
        };
        pool.persist_path = Some(path.clone());
        if meta.ops_since_checkpoint >= CHECKPOINT_EVERY {
            let snap = pool.snapshot();
            let current = indexed_envelopes(&snap);
            checkpoint_mempool(&path, &snap, &mut meta, &current)?;
        }
        pool.persist_wal = Some(Mutex::new(meta));
        Ok(pool)
    }

    pub fn set_persist_path(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref().to_path_buf();
        let wal_path = path.with_extension("wal");
        self.persist_path = Some(path);
        self.persist_wal = open_mempool_wal(&wal_path).ok().map(|file| {
            Mutex::new(MempoolWalMeta {
                file,
                ops_since_checkpoint: 0,
                live: fingerprints(&self.snapshot()),
            })
        });
    }

    pub(crate) fn persist_best_effort(&self) {
        let Some(path) = self.persist_path.as_ref() else {
            return;
        };
        let Some(wal) = self.persist_wal.as_ref() else {
            if let Err(err) = write_snapshot(path, &self.snapshot()) {
                warn!(error = %err, path = %path.display(), "durable mempool flush failed");
            }
            return;
        };
        let snap = self.snapshot();
        let current = indexed_envelopes(&snap);
        let mut meta = match wal.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let removed: Vec<Hash> = meta
            .live
            .keys()
            .filter(|id| !current.contains_key(*id))
            .copied()
            .collect();
        if !removed.is_empty() {
            if let Err(err) = checkpoint_mempool(path, &snap, &mut meta, &current) {
                warn!(error = %err, path = %path.display(), "durable mempool checkpoint failed");
            }
            return;
        }
        for (id, env) in &current {
            let fp = envelope_fp(env);
            if meta.live.get(id) == Some(&fp) {
                continue;
            }
            if let Err(err) = append_mempool_wal(&mut meta.file, &MempoolWalOp::Upsert(env.clone()))
            {
                warn!(error = %err, "durable mempool WAL append failed");
                return;
            }
            meta.live.insert(*id, fp);
            meta.ops_since_checkpoint = meta.ops_since_checkpoint.saturating_add(1);
        }
        if meta.ops_since_checkpoint >= CHECKPOINT_EVERY {
            if let Err(err) = checkpoint_mempool(path, &snap, &mut meta, &current) {
                warn!(error = %err, path = %path.display(), "durable mempool checkpoint failed");
            }
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

fn indexed_envelopes(snap: &DurableMempoolV1) -> HashMap<Hash, DurableEnvelope> {
    let mut out = HashMap::new();
    let mut push = |env: DurableEnvelope| {
        out.insert(envelope_id(&env), env);
    };
    for tx in &snap.tlt {
        push(DurableEnvelope::Tlt(tx.clone()));
    }
    for tx in &snap.covenants {
        push(DurableEnvelope::Covenant(tx.clone()));
    }
    for tx in &snap.accounts {
        push(DurableEnvelope::Account(tx.clone()));
    }
    for tx in &snap.stakes {
        push(DurableEnvelope::Stake(tx.clone()));
    }
    for tx in &snap.ovl {
        push(DurableEnvelope::Ovl(tx.clone()));
    }
    for tx in &snap.ovl_raw {
        push(DurableEnvelope::OvlRaw(tx.clone()));
    }
    for tx in &snap.payments {
        push(DurableEnvelope::Payment(tx.clone()));
    }
    for tx in &snap.policies {
        push(DurableEnvelope::Policy(tx.clone()));
    }
    for tx in &snap.preauths {
        push(DurableEnvelope::Preauth(tx.clone()));
    }
    for tx in &snap.regular_keys {
        push(DurableEnvelope::RegularKey(tx.clone()));
    }
    for tx in &snap.signer_lists {
        push(DurableEnvelope::SignerList(tx.clone()));
    }
    for tx in &snap.tickets {
        push(DurableEnvelope::Ticket(tx.clone()));
    }
    for tx in &snap.escrow_creates {
        push(DurableEnvelope::EscrowCreate(tx.clone()));
    }
    for tx in &snap.escrow_finishes {
        push(DurableEnvelope::EscrowFinish(tx.clone()));
    }
    for tx in &snap.escrow_cancels {
        push(DurableEnvelope::EscrowCancel(tx.clone()));
    }
    for tx in &snap.check_creates {
        push(DurableEnvelope::CheckCreate(tx.clone()));
    }
    for tx in &snap.check_cashes {
        push(DurableEnvelope::CheckCash(tx.clone()));
    }
    for tx in &snap.check_cancels {
        push(DurableEnvelope::CheckCancel(tx.clone()));
    }
    for tx in &snap.channel_creates {
        push(DurableEnvelope::ChannelCreate(tx.clone()));
    }
    for tx in &snap.channel_funds {
        push(DurableEnvelope::ChannelFund(tx.clone()));
    }
    for tx in &snap.channel_claims {
        push(DurableEnvelope::ChannelClaim(tx.clone()));
    }
    for tx in &snap.channel_closes {
        push(DurableEnvelope::ChannelClose(tx.clone()));
    }
    for tx in &snap.trust_lines {
        push(DurableEnvelope::TrustLine(tx.clone()));
    }
    for tx in &snap.issued_transfers {
        push(DurableEnvelope::IssuedTransfer(tx.clone()));
    }
    for tx in &snap.asset_policies {
        push(DurableEnvelope::AssetPolicy(tx.clone()));
    }
    for tx in &snap.issuer_controls {
        push(DurableEnvelope::IssuerControl(tx.clone()));
    }
    for tx in &snap.clawbacks {
        push(DurableEnvelope::Clawback(tx.clone()));
    }
    for tx in &snap.offer_creates {
        push(DurableEnvelope::OfferCreate(tx.clone()));
    }
    for tx in &snap.offer_cancels {
        push(DurableEnvelope::OfferCancel(tx.clone()));
    }
    for tx in &snap.data_commitments {
        push(DurableEnvelope::DataCommitment(tx.clone()));
    }
    for tx in &snap.multisign {
        push(DurableEnvelope::Multisign(tx.clone()));
    }
    for tx in &snap.passports {
        push(DurableEnvelope::Passport(tx.clone()));
    }
    for tx in &snap.hubs {
        push(DurableEnvelope::Hub(tx.clone()));
    }
    for tx in &snap.grants {
        push(DurableEnvelope::Grant(tx.clone()));
    }
    for tx in &snap.missions {
        push(DurableEnvelope::Mission(tx.clone()));
    }
    for tx in &snap.treasuries {
        push(DurableEnvelope::Treasury(tx.clone()));
    }
    for tx in &snap.vestings {
        push(DurableEnvelope::Vesting(tx.clone()));
    }
    out
}

fn fingerprints(snap: &DurableMempoolV1) -> HashMap<Hash, u32> {
    indexed_envelopes(snap)
        .into_iter()
        .map(|(id, env)| (id, envelope_fp(&env)))
        .collect()
}

fn envelope_id(env: &DurableEnvelope) -> Hash {
    match env {
        DurableEnvelope::Tlt(tx) => tx.tx.tx_id(),
        DurableEnvelope::Covenant(tx) => tx.tx.tx_id(),
        DurableEnvelope::Account(tx) => tx.transfer_id(),
        DurableEnvelope::Stake(tx) => tx.stake_tx_id(),
        DurableEnvelope::Ovl(tx) | DurableEnvelope::OvlRaw(tx) => tx.tx_id(),
        DurableEnvelope::Payment(tx) => tx.tx.payment_id(),
        DurableEnvelope::Policy(tx) => tx.policy_tx_id(),
        DurableEnvelope::Preauth(tx) => tx.preauth_tx_id(),
        DurableEnvelope::RegularKey(tx) => tx.regular_key_tx_id(),
        DurableEnvelope::SignerList(tx) => tx.signer_list_tx_id(),
        DurableEnvelope::Ticket(tx) => tx.ticket_create_tx_id(),
        DurableEnvelope::EscrowCreate(tx) => tx.escrow_id(),
        DurableEnvelope::EscrowFinish(tx) => tx.finish_tx_id(),
        DurableEnvelope::EscrowCancel(tx) => tx.cancel_tx_id(),
        DurableEnvelope::CheckCreate(tx) => tx.check_id(),
        DurableEnvelope::CheckCash(tx) => tx.cash_tx_id(),
        DurableEnvelope::CheckCancel(tx) => tx.cancel_tx_id(),
        DurableEnvelope::ChannelCreate(tx) => tx.channel_id(),
        DurableEnvelope::ChannelFund(tx) => tx.fund_tx_id(),
        DurableEnvelope::ChannelClaim(tx) => tx.claim_tx_id(),
        DurableEnvelope::ChannelClose(tx) => tx.close_tx_id(),
        DurableEnvelope::TrustLine(tx) => tx.tx.trust_line_set_tx_id(),
        DurableEnvelope::IssuedTransfer(tx) => tx.issued_transfer_tx_id(),
        DurableEnvelope::AssetPolicy(tx) => tx.policy_set_tx_id(),
        DurableEnvelope::IssuerControl(tx) => tx.issuer_control_tx_id(),
        DurableEnvelope::Clawback(tx) => tx.clawback_tx_id(),
        DurableEnvelope::OfferCreate(tx) => tx.offer_id(),
        DurableEnvelope::OfferCancel(tx) => tx.cancel_tx_id(),
        DurableEnvelope::DataCommitment(tx) => tx.authorization_id(),
        DurableEnvelope::Multisign(tx) => tx.body_commitment_id(),
        DurableEnvelope::Passport(tx) => tx.attestation_id(),
        DurableEnvelope::Hub(tx) => tx.registration_id(),
        DurableEnvelope::Grant(tx) => tx.registration_id(),
        DurableEnvelope::Mission(tx) => tx.registration_id(),
        DurableEnvelope::Treasury(tx) => tx.disbursement_id(),
        DurableEnvelope::Vesting(tx) => tx.unlock_id(),
    }
}

fn envelope_fp(env: &DurableEnvelope) -> u32 {
    fnv1a32(&borsh::to_vec(env).unwrap_or_default())
}

fn apply_mempool_wal_op(pool: &mut Mempool, op: MempoolWalOp) {
    let MempoolWalOp::Upsert(env) = op;
    match env {
        DurableEnvelope::Tlt(item) => {
            let _ = if item.rbf {
                pool.admit_priced_opt_in(item.tx, item.fee, true)
            } else {
                pool.admit_priced(item.tx, item.fee)
            };
        }
        DurableEnvelope::Covenant(item) => {
            let _ = pool.admit_covenant(item.tx, item.fee);
        }
        DurableEnvelope::Account(tx) => {
            let _ = pool.admit_account(tx);
        }
        DurableEnvelope::Stake(tx) => {
            let _ = pool.admit_stake(tx);
        }
        DurableEnvelope::Ovl(tx) => {
            let _ = pool.admit_ovl_execution(tx);
        }
        DurableEnvelope::OvlRaw(tx) => {
            let _ = pool.admit_ovl_raw_execution(tx);
        }
        DurableEnvelope::Payment(item) => {
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
        DurableEnvelope::Policy(tx) => {
            let _ = pool.admit_drc_policy(tx);
        }
        DurableEnvelope::Preauth(tx) => {
            let _ = pool.admit_drc_deposit_preauth(tx);
        }
        DurableEnvelope::RegularKey(tx) => {
            let _ = pool.admit_drc_regular_key(tx);
        }
        DurableEnvelope::SignerList(tx) => {
            let _ = pool.admit_drc_signer_list(tx);
        }
        DurableEnvelope::Ticket(tx) => {
            let _ = pool.admit_drc_ticket_create(tx);
        }
        DurableEnvelope::EscrowCreate(tx) => {
            let _ = pool.admit_drc_escrow_create(tx);
        }
        DurableEnvelope::EscrowFinish(tx) => {
            let _ = pool.admit_drc_escrow_finish(tx);
        }
        DurableEnvelope::EscrowCancel(tx) => {
            let _ = pool.admit_drc_escrow_cancel(tx);
        }
        DurableEnvelope::CheckCreate(tx) => {
            let _ = pool.admit_drc_check_create(tx);
        }
        DurableEnvelope::CheckCash(tx) => {
            let _ = pool.admit_drc_check_cash(tx);
        }
        DurableEnvelope::CheckCancel(tx) => {
            let _ = pool.admit_drc_check_cancel(tx);
        }
        DurableEnvelope::ChannelCreate(tx) => {
            let _ = pool.admit_drc_payment_channel_create(tx);
        }
        DurableEnvelope::ChannelFund(tx) => {
            let _ = pool.admit_drc_payment_channel_fund(tx);
        }
        DurableEnvelope::ChannelClaim(tx) => {
            let _ = pool.admit_drc_payment_channel_claim(tx);
        }
        DurableEnvelope::ChannelClose(tx) => {
            let _ = pool.admit_drc_payment_channel_close(tx);
        }
        DurableEnvelope::TrustLine(item) => {
            let _ =
                pool.admit_drc_trust_line_set(item.tx, item.pending_create, item.pending_delete);
        }
        DurableEnvelope::IssuedTransfer(tx) => {
            let _ = pool.admit_drc_issued_transfer(tx);
        }
        DurableEnvelope::AssetPolicy(tx) => {
            let _ = pool.admit_drc_issued_asset_policy_set(tx, Vec::new());
        }
        DurableEnvelope::IssuerControl(tx) => {
            let _ = pool.admit_drc_trust_line_issuer_control(tx);
        }
        DurableEnvelope::Clawback(tx) => {
            let _ = pool.admit_drc_issued_clawback(tx);
        }
        DurableEnvelope::OfferCreate(tx) => {
            let _ = pool.admit_drc_offer_create(tx);
        }
        DurableEnvelope::OfferCancel(tx) => {
            let _ = pool.admit_drc_offer_cancel(tx);
        }
        DurableEnvelope::DataCommitment(tx) => {
            let _ = pool.admit_data_commitment(tx);
        }
        DurableEnvelope::Multisign(tx) => {
            let _ = pool.admit_drc_multisign_attachment(tx);
        }
        DurableEnvelope::Passport(tx) => {
            let _ = pool.admit_passport_attestation(tx);
        }
        DurableEnvelope::Hub(tx) => {
            let _ = pool.admit_hub_registration(tx);
        }
        DurableEnvelope::Grant(tx) => {
            let _ = pool.admit_grant_registration(tx);
        }
        DurableEnvelope::Mission(tx) => {
            let _ = pool.admit_mission_registration(tx);
        }
        DurableEnvelope::Treasury(tx) => {
            let _ = pool.admit_treasury_disbursement(tx);
        }
        DurableEnvelope::Vesting(tx) => {
            let _ = pool.admit_vesting_unlock(tx);
        }
    }
}

fn append_mempool_wal(file: &mut File, op: &MempoolWalOp) -> Result<(), P2pError> {
    let payload = borsh::to_vec(op).map_err(|e| P2pError::Decode(e.to_string()))?;
    let len = u32::try_from(payload.len()).map_err(|_| P2pError::Decode("WAL too large".into()))?;
    file.write_all(&len.to_le_bytes())
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    file.write_all(&payload)
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    file.write_all(&fnv1a32(&payload).to_le_bytes())
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    file.flush().map_err(|e| P2pError::Decode(e.to_string()))?;
    let _ = file.sync_data();
    Ok(())
}

fn replay_mempool_wal(path: &Path) -> Result<Vec<MempoolWalOp>, P2pError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut file = File::open(path).map_err(|e| P2pError::Decode(e.to_string()))?;
    let mut magic = [0u8; 8];
    match file.read_exact(&mut magic) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(Vec::new()),
        Err(err) => return Err(P2pError::Decode(err.to_string())),
    }
    if &magic != WAL_MAGIC {
        return Err(P2pError::Decode(
            "portable mempool WAL is missing AGORAW01 magic".into(),
        ));
    }
    let mut ops = Vec::new();
    loop {
        let mut len_buf = [0u8; 4];
        match file.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(P2pError::Decode(err.to_string())),
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        if file.read_exact(&mut payload).is_err() {
            break;
        }
        let mut crc = [0u8; 4];
        if file.read_exact(&mut crc).is_err() {
            break;
        }
        if u32::from_le_bytes(crc) != fnv1a32(&payload) {
            break;
        }
        match MempoolWalOp::try_from_slice(&payload) {
            Ok(op) => ops.push(op),
            Err(_) => break,
        }
    }
    Ok(ops)
}

fn checkpoint_mempool(
    path: &Path,
    snap: &DurableMempoolV1,
    meta: &mut MempoolWalMeta,
    current: &HashMap<Hash, DurableEnvelope>,
) -> Result<(), P2pError> {
    write_snapshot(path, snap)?;
    truncate_mempool_wal(&mut meta.file)?;
    meta.live = current
        .iter()
        .map(|(id, env)| (*id, envelope_fp(env)))
        .collect();
    meta.ops_since_checkpoint = 0;
    Ok(())
}

fn open_mempool_wal(path: &Path) -> Result<File, P2pError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| P2pError::Decode(e.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    if file
        .metadata()
        .map_err(|e| P2pError::Decode(e.to_string()))?
        .len()
        == 0
    {
        file.write_all(WAL_MAGIC)
            .map_err(|e| P2pError::Decode(e.to_string()))?;
        let _ = file.sync_all();
    } else {
        file.seek(SeekFrom::End(0))
            .map_err(|e| P2pError::Decode(e.to_string()))?;
    }
    Ok(file)
}

fn truncate_mempool_wal(file: &mut File) -> Result<(), P2pError> {
    file.set_len(0)
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    file.write_all(WAL_MAGIC)
        .map_err(|e| P2pError::Decode(e.to_string()))?;
    let _ = file.sync_all();
    Ok(())
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
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
            assert!(path.with_extension("wal").is_file());
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
    fn mempool_wal_avoids_rewriting_snapshot_on_each_admit() {
        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        let wal = mempool_wal_path(dir.path());
        let first = signed_spend(0, 1);
        let second = signed_spend(1, 2);
        {
            let mut pool = Mempool::open_persistent(&path, 16).unwrap();
            pool.admit_priced(first.clone(), 3).unwrap();
            assert!(
                !path.exists(),
                "first admit must not write a full AGORAMP1 snapshot"
            );
            assert!(wal.is_file());
            let wal_len = std::fs::metadata(&wal).unwrap().len();
            pool.admit_priced(second.clone(), 4).unwrap();
            assert!(!path.exists());
            assert!(std::fs::metadata(&wal).unwrap().len() > wal_len);
        }
        let pool = Mempool::open_persistent(&path, 16).unwrap();
        assert!(pool.contains(&first.tx_id()));
        assert!(pool.contains(&second.tx_id()));
        assert_eq!(pool.fee_of(&second.tx_id()), Some(4));
    }

    #[test]
    fn mempool_wal_torn_tail_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        let wal = mempool_wal_path(dir.path());
        let tx = signed_spend(0, 1);
        let id = tx.tx_id();
        {
            let mut pool = Mempool::open_persistent(&path, 16).unwrap();
            pool.admit_priced(tx, 5).unwrap();
        }
        let mut bytes = std::fs::read(&wal).unwrap();
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        std::fs::write(&wal, bytes).unwrap();
        let pool = Mempool::open_persistent(&path, 16).unwrap();
        assert!(pool.contains(&id));
        assert_eq!(pool.fee_of(&id), Some(5));
    }

    #[test]
    fn mempool_wal_checkpoints_and_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let path = mempool_persist_path(dir.path());
        let wal = mempool_wal_path(dir.path());
        {
            let mut pool = Mempool::open_persistent(&path, 64).unwrap();
            for i in 0u32..32 {
                pool.admit_priced(signed_spend(i, u64::from(i) + 1), 1)
                    .unwrap();
            }
            let wal_len = std::fs::metadata(&wal).unwrap().len();
            assert_eq!(
                wal_len,
                WAL_MAGIC.len() as u64,
                "checkpoint must truncate the WAL to magic"
            );
            assert!(path.is_file());
            pool.admit_priced(signed_spend(32, 33), 1).unwrap();
            assert!(std::fs::metadata(&wal).unwrap().len() > WAL_MAGIC.len() as u64);
        }
        let pool = Mempool::open_persistent(&path, 64).unwrap();
        assert!(pool.contains(&signed_spend(32, 33).tx_id()));
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
