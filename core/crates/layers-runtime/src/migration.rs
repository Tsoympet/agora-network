//! Deterministic, read-only export of historical layer checkpoints.
//!
//! This module deliberately stops at an auditable dry-run artifact. It never
//! writes Trident state or creates claim entitlements.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use agora_bridge_sdk::{BridgeDirection, MessageStatus, ATTESTOR_BOND_ESCROW};
use agora_ovolos_rollup::SEQUENCER_BOND_ESCROW;
use agora_types::{Address, Hash};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::LayersCheckpoint;

const SNAPSHOT_VERSION: u32 = 1;
const LEAF_DOMAIN: &[u8] = b"agora-trident-migration-claim-leaf-v1";
const NODE_DOMAIN: &[u8] = b"agora-trident-migration-claim-node-v1";
const EMPTY_DOMAIN: &[u8] = b"agora-trident-migration-empty-v1";

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("checkpoint: {0}")]
    Checkpoint(String),
    #[error("invalid source checkpoint: {0}")]
    InvalidSource(String),
    #[error("snapshot: {0}")]
    Snapshot(String),
    #[error("snapshot root mismatch: expected {expected}, computed {computed}")]
    RootMismatch { expected: String, computed: String },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum MigrationAsset {
    Ovl,
    Drc,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AllocationKind {
    Liquid,
    Stake,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MigrationAllocation {
    pub asset: MigrationAsset,
    pub kind: AllocationKind,
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MigrationSource {
    pub checkpoint_version: u32,
    pub head_state_root: String,
    pub next_sequence: u64,
    pub ovl_tip_hash: String,
    pub ovl_tip_height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct DistrictBalanceAudit {
    pub district: String,
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LockAudit {
    pub domain: String,
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MessageAudit {
    pub id: String,
    pub direction: String,
    pub status: String,
    pub source: String,
    pub destination: String,
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub nonce: u64,
    pub destination_tag: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EvmAccountAudit {
    pub address: String,
    pub balance_hex: String,
    pub nonce: u64,
    pub code_hash: String,
    pub code_bytes: u64,
    pub storage_slots: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SupplyAudit {
    pub source_minted: u64,
    pub source_ledger: u64,
    pub proposed_claims: u64,
    pub retired_or_burned: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MigrationAudit {
    pub ready_for_claim_design: bool,
    pub ovl: SupplyAudit,
    pub drc: SupplyAudit,
    pub lock_total: u64,
    pub pending_message_count: u64,
    pub quarantined_evm_account_count: u64,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MigrationSnapshotBody {
    pub version: u32,
    pub maturity: String,
    pub claim_activation: bool,
    pub source: MigrationSource,
    pub allocations: Vec<MigrationAllocation>,
    pub district_balances: Vec<DistrictBalanceAudit>,
    pub locks: Vec<LockAudit>,
    pub messages: Vec<MessageAudit>,
    pub quarantined_evm_accounts: Vec<EvmAccountAudit>,
    pub claim_root: String,
    pub audit: MigrationAudit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationSnapshot {
    #[serde(flatten)]
    pub body: MigrationSnapshotBody,
    pub snapshot_root: String,
}

pub fn export_migration_snapshot(
    checkpoint: &LayersCheckpoint,
) -> Result<MigrationSnapshot, MigrationError> {
    validate_checkpoint_header(checkpoint)?;

    let mut blockers = Vec::new();
    let mut allocations = Vec::new();

    let ovl_balances = unique_address_amounts(&checkpoint.ovl_balances, "OVL balance")?;
    let ovl_bonds = unique_address_amounts(&checkpoint.sequencer_bonds, "OVL bond")?;
    let ovl_bond_total = checked_sum(ovl_bonds.values().copied(), "OVL bond total")?;
    let ovl_escrow = ovl_balances
        .get(&SEQUENCER_BOND_ESCROW)
        .copied()
        .unwrap_or(0);
    if ovl_escrow != ovl_bond_total {
        blockers.push(format!(
            "OVL sequencer escrow mismatch: ledger={ovl_escrow}, bonds={ovl_bond_total}"
        ));
    }
    for (address, amount) in &ovl_balances {
        if *address != SEQUENCER_BOND_ESCROW && *amount > 0 {
            allocations.push(allocation(
                MigrationAsset::Ovl,
                AllocationKind::Liquid,
                *address,
                *amount,
            ));
        }
    }
    for (address, amount) in &ovl_bonds {
        if *amount > 0 {
            allocations.push(allocation(
                MigrationAsset::Ovl,
                AllocationKind::Stake,
                *address,
                *amount,
            ));
        }
    }

    let mut district_balances = Vec::new();
    let mut drc_by_address = BTreeMap::<Address, u64>::new();
    let mut seen_drc = BTreeSet::new();
    let mut drc_ledger_total = 0u64;
    let hub = checkpoint.bridge.attestor_hub.as_str();
    let mut drc_escrow = 0u64;
    for (district, address, amount) in &checkpoint.bridge.balances {
        if !seen_drc.insert((district.clone(), *address)) {
            return Err(MigrationError::InvalidSource(format!(
                "duplicate DRC balance {district}/{}",
                address.to_hex()
            )));
        }
        drc_ledger_total = drc_ledger_total
            .checked_add(*amount)
            .ok_or_else(|| MigrationError::InvalidSource("DRC ledger total overflow".into()))?;
        district_balances.push(DistrictBalanceAudit {
            district: district.clone(),
            address: address.to_hex(),
            amount: *amount,
        });
        if district == hub && *address == ATTESTOR_BOND_ESCROW {
            drc_escrow = *amount;
        } else {
            let entry = drc_by_address.entry(*address).or_default();
            *entry = entry.checked_add(*amount).ok_or_else(|| {
                MigrationError::InvalidSource(format!(
                    "DRC aggregate overflow for {}",
                    address.to_hex()
                ))
            })?;
        }
    }
    district_balances.sort_by(|a, b| {
        (&a.district, &a.address, a.amount).cmp(&(&b.district, &b.address, b.amount))
    });

    let drc_bonds = unique_address_amounts(&checkpoint.bridge.attestor_bonds, "DRC attestor bond")?;
    let drc_bond_total = checked_sum(drc_bonds.values().copied(), "DRC bond total")?;
    if drc_escrow != drc_bond_total {
        blockers.push(format!(
            "DRC attestor escrow mismatch: ledger={drc_escrow}, bonds={drc_bond_total}"
        ));
    }
    for (address, amount) in drc_by_address {
        if amount > 0 {
            allocations.push(allocation(
                MigrationAsset::Drc,
                AllocationKind::Liquid,
                address,
                amount,
            ));
        }
    }
    for (address, amount) in &drc_bonds {
        if *amount > 0 {
            allocations.push(allocation(
                MigrationAsset::Drc,
                AllocationKind::Stake,
                *address,
                *amount,
            ));
        }
    }
    allocations.sort();

    let ovl_ledger_total = checked_sum(ovl_balances.values().copied(), "OVL ledger total")?;
    if ovl_ledger_total > checkpoint.ovl_minted {
        return Err(MigrationError::InvalidSource(format!(
            "OVL ledger {ovl_ledger_total} exceeds minted {}",
            checkpoint.ovl_minted
        )));
    }
    if drc_ledger_total > checkpoint.bridge.minted {
        return Err(MigrationError::InvalidSource(format!(
            "DRC ledger {drc_ledger_total} exceeds minted {}",
            checkpoint.bridge.minted
        )));
    }

    let ovl_claims = allocation_total(&allocations, MigrationAsset::Ovl)?;
    let drc_claims = allocation_total(&allocations, MigrationAsset::Drc)?;
    if ovl_claims != ovl_ledger_total {
        blockers.push(format!(
            "OVL proposed claims {ovl_claims} do not equal ledger {ovl_ledger_total}"
        ));
    }
    if drc_claims != drc_ledger_total {
        blockers.push(format!(
            "DRC proposed claims {drc_claims} do not equal ledger {drc_ledger_total}"
        ));
    }

    let locks = lock_audit(checkpoint)?;
    let lock_total = checked_sum(locks.iter().map(|lock| lock.amount), "lock total")?;
    if lock_total > 0 {
        blockers.push(format!(
            "{lock_total} DRC base units remain represented by historical bridge locks"
        ));
    }

    let messages = message_audit(checkpoint)?;
    let pending_message_count = messages
        .iter()
        .filter(|message| matches!(message.status.as_str(), "locked" | "paid"))
        .count() as u64;
    if pending_message_count > 0 {
        blockers.push(format!(
            "{pending_message_count} bridge messages require freeze-height resolution"
        ));
    }

    let quarantined_evm_accounts = evm_audit(checkpoint)?;
    if !quarantined_evm_accounts.is_empty() {
        blockers.push(format!(
            "{} EVM accounts at the rollup head require an explicit balance/state policy",
            quarantined_evm_accounts.len()
        ));
    }

    let claim_root = merkle_root(&allocations);
    let audit = MigrationAudit {
        ready_for_claim_design: blockers.is_empty(),
        ovl: SupplyAudit {
            source_minted: checkpoint.ovl_minted,
            source_ledger: ovl_ledger_total,
            proposed_claims: ovl_claims,
            retired_or_burned: checkpoint.ovl_minted - ovl_ledger_total,
        },
        drc: SupplyAudit {
            source_minted: checkpoint.bridge.minted,
            source_ledger: drc_ledger_total,
            proposed_claims: drc_claims,
            retired_or_burned: checkpoint.bridge.minted - drc_ledger_total,
        },
        lock_total,
        pending_message_count,
        quarantined_evm_account_count: quarantined_evm_accounts.len() as u64,
        blockers,
    };
    let body = MigrationSnapshotBody {
        version: SNAPSHOT_VERSION,
        maturity: "Scaffold".into(),
        claim_activation: false,
        source: MigrationSource {
            checkpoint_version: checkpoint.version,
            head_state_root: checkpoint.head_state_root.clone(),
            next_sequence: checkpoint.next_sequence,
            ovl_tip_hash: checkpoint.ovl_tip_hash.clone(),
            ovl_tip_height: checkpoint.ovl_tip_height,
        },
        allocations,
        district_balances,
        locks,
        messages,
        quarantined_evm_accounts,
        claim_root: claim_root.to_hex(),
        audit,
    };
    let snapshot_root = Hash::hash_borsh(&body).to_hex();
    Ok(MigrationSnapshot {
        body,
        snapshot_root,
    })
}

pub fn verify_migration_snapshot(snapshot: &MigrationSnapshot) -> Result<(), MigrationError> {
    if snapshot.body.version != SNAPSHOT_VERSION {
        return Err(MigrationError::Snapshot(format!(
            "unsupported version {}",
            snapshot.body.version
        )));
    }
    if snapshot.body.claim_activation {
        return Err(MigrationError::Snapshot(
            "dry-run snapshots cannot activate claims".into(),
        ));
    }
    if snapshot
        .body
        .allocations
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err(MigrationError::Snapshot(
            "allocations must be strictly sorted and unique".into(),
        ));
    }
    let claim_root = merkle_root(&snapshot.body.allocations).to_hex();
    if claim_root != snapshot.body.claim_root {
        return Err(MigrationError::RootMismatch {
            expected: snapshot.body.claim_root.clone(),
            computed: claim_root,
        });
    }
    let computed = Hash::hash_borsh(&snapshot.body).to_hex();
    if computed != snapshot.snapshot_root {
        return Err(MigrationError::RootMismatch {
            expected: snapshot.snapshot_root.clone(),
            computed,
        });
    }
    Ok(())
}

pub fn load_and_verify_migration_snapshot(
    path: impl AsRef<Path>,
) -> Result<MigrationSnapshot, MigrationError> {
    let bytes = fs::read(path.as_ref())
        .map_err(|error| MigrationError::Snapshot(format!("read: {error}")))?;
    let snapshot = serde_json::from_slice::<MigrationSnapshot>(&bytes)
        .map_err(|error| MigrationError::Snapshot(format!("decode: {error}")))?;
    verify_migration_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_checkpoint_header(checkpoint: &LayersCheckpoint) -> Result<(), MigrationError> {
    if checkpoint.version != 1 {
        return Err(MigrationError::InvalidSource(format!(
            "unsupported checkpoint version {}",
            checkpoint.version
        )));
    }
    for (label, value) in [
        ("head_state_root", checkpoint.head_state_root.as_str()),
        ("ovl_tip_hash", checkpoint.ovl_tip_hash.as_str()),
    ] {
        if Hash::from_hex(value).is_none() {
            return Err(MigrationError::InvalidSource(format!(
                "{label} is not a 32-byte hex hash"
            )));
        }
    }
    Ok(())
}

fn allocation(
    asset: MigrationAsset,
    kind: AllocationKind,
    address: Address,
    amount: u64,
) -> MigrationAllocation {
    MigrationAllocation {
        asset,
        kind,
        address: address.to_hex(),
        amount,
    }
}

fn unique_address_amounts(
    entries: &[(Address, u64)],
    label: &str,
) -> Result<BTreeMap<Address, u64>, MigrationError> {
    let mut result = BTreeMap::new();
    for (address, amount) in entries {
        if result.insert(*address, *amount).is_some() {
            return Err(MigrationError::InvalidSource(format!(
                "duplicate {label} {}",
                address.to_hex()
            )));
        }
    }
    Ok(result)
}

fn checked_sum(values: impl IntoIterator<Item = u64>, label: &str) -> Result<u64, MigrationError> {
    values.into_iter().try_fold(0u64, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| MigrationError::InvalidSource(format!("{label} overflow")))
    })
}

fn allocation_total(
    allocations: &[MigrationAllocation],
    asset: MigrationAsset,
) -> Result<u64, MigrationError> {
    checked_sum(
        allocations
            .iter()
            .filter(|allocation| allocation.asset == asset)
            .map(|allocation| allocation.amount),
        "allocation total",
    )
}

fn lock_audit(checkpoint: &LayersCheckpoint) -> Result<Vec<LockAudit>, MigrationError> {
    let mut seen = BTreeSet::new();
    let mut locks = Vec::new();
    for (domain, address, amount) in &checkpoint.bridge.locks {
        if !seen.insert((domain.clone(), *address)) {
            return Err(MigrationError::InvalidSource(format!(
                "duplicate lock {domain}/{}",
                address.to_hex()
            )));
        }
        if *amount > 0 {
            locks.push(LockAudit {
                domain: domain.clone(),
                address: address.to_hex(),
                amount: *amount,
            });
        }
    }
    locks.sort_by(|a, b| (&a.domain, &a.address, a.amount).cmp(&(&b.domain, &b.address, b.amount)));
    Ok(locks)
}

fn message_audit(checkpoint: &LayersCheckpoint) -> Result<Vec<MessageAudit>, MigrationError> {
    let mut seen = BTreeSet::new();
    let mut messages = Vec::new();
    for (id, message, status) in &checkpoint.bridge.messages {
        let parsed = Hash::from_hex(id)
            .ok_or_else(|| MigrationError::InvalidSource(format!("invalid message id {id}")))?;
        if !seen.insert(parsed) {
            return Err(MigrationError::InvalidSource(format!(
                "duplicate message id {id}"
            )));
        }
        if message.id() != parsed {
            return Err(MigrationError::InvalidSource(format!(
                "message id does not match payload: {id}"
            )));
        }
        messages.push(MessageAudit {
            id: parsed.to_hex(),
            direction: direction_name(message.direction).into(),
            status: status_name(*status).into(),
            source: message.source_district.clone(),
            destination: message.dest_district.clone(),
            sender: message.sender.to_hex(),
            recipient: message.recipient.to_hex(),
            amount: message.amount.as_base_units(),
            nonce: message.nonce,
            destination_tag: message.destination_tag,
        });
    }
    messages.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(messages)
}

fn direction_name(direction: BridgeDirection) -> &'static str {
    match direction {
        BridgeDirection::LockAndMint => "lock_and_mint",
        BridgeDirection::BurnAndUnlock => "burn_and_unlock",
        BridgeDirection::Payment => "payment",
    }
}

fn status_name(status: MessageStatus) -> &'static str {
    match status {
        MessageStatus::Locked => "locked",
        MessageStatus::Claimed => "claimed",
        MessageStatus::Unlocked => "unlocked",
        MessageStatus::Paid => "paid",
        MessageStatus::Finalized => "finalized",
    }
}

fn evm_audit(checkpoint: &LayersCheckpoint) -> Result<Vec<EvmAccountAudit>, MigrationError> {
    let Some((_, accounts)) = checkpoint
        .revm_snapshots
        .iter()
        .find(|(root, _)| root == &checkpoint.head_state_root)
    else {
        if checkpoint.head_state_root == Hash::ZERO.to_hex() {
            return Ok(Vec::new());
        }
        return Err(MigrationError::InvalidSource(
            "rollup head is absent from EVM snapshots".into(),
        ));
    };
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for account in accounts {
        let address = Address(account.address);
        if !seen.insert(address) {
            return Err(MigrationError::InvalidSource(format!(
                "duplicate EVM account {} at rollup head",
                address.to_hex()
            )));
        }
        let relevant = account.balance.iter().any(|byte| *byte != 0)
            || account.nonce != 0
            || !account.code.is_empty()
            || !account.storage.is_empty();
        if relevant {
            result.push(EvmAccountAudit {
                address: address.to_hex(),
                balance_hex: format!("0x{}", hex::encode(account.balance)),
                nonce: account.nonce,
                code_hash: hex::encode(account.code_hash),
                code_bytes: account.code.len() as u64,
                storage_slots: account.storage.len() as u64,
            });
        }
    }
    result.sort_by(|a, b| a.address.cmp(&b.address));
    Ok(result)
}

fn merkle_root(allocations: &[MigrationAllocation]) -> Hash {
    if allocations.is_empty() {
        return Hash::hash_bytes(EMPTY_DOMAIN);
    }
    let mut level = allocations
        .iter()
        .map(|allocation| {
            let mut bytes = LEAF_DOMAIN.to_vec();
            bytes.extend(
                borsh::to_vec(allocation)
                    .expect("migration allocation serialization is infallible"),
            );
            Hash::hash_bytes(&bytes)
        })
        .collect::<Vec<_>>();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = pair.get(1).copied().unwrap_or(left);
            let mut bytes = NODE_DOMAIN.to_vec();
            bytes.extend_from_slice(left.as_bytes());
            bytes.extend_from_slice(right.as_bytes());
            next.push(Hash::hash_bytes(&bytes));
        }
        level = next;
    }
    level[0]
}

#[cfg(test)]
mod tests {
    use agora_bridge_sdk::{BridgeCheckpoint, BridgeMessage};

    use super::*;

    fn checkpoint() -> LayersCheckpoint {
        let alice = Address([1; 20]);
        let bob = Address([2; 20]);
        LayersCheckpoint {
            version: 1,
            head_state_root: Hash::ZERO.to_hex(),
            next_sequence: 7,
            ovl_tip_hash: Hash([3; 32]).to_hex(),
            ovl_tip_height: 9,
            ovl_balances: vec![(alice, 60), (SEQUENCER_BOND_ESCROW, 40)],
            ovl_minted: 110,
            sequencer_bonds: vec![(bob, 40)],
            revm_snapshots: Vec::new(),
            bridge: BridgeCheckpoint {
                balances: vec![
                    ("hub".into(), alice, 30),
                    ("east".into(), alice, 20),
                    ("hub".into(), ATTESTOR_BOND_ESCROW, 10),
                ],
                minted: 60,
                max_supply: 1_000,
                locks: Vec::new(),
                messages: Vec::new(),
                tips: Vec::new(),
                tag_owners: Vec::new(),
                tag_payments: Vec::new(),
                attestor_bonds: vec![(bob, 10)],
                attestor_hub: "hub".into(),
                attestor_min_bond: 1,
            },
            l2_mempool: Vec::new(),
        }
    }

    #[test]
    fn export_is_deterministic_and_conserves_ledgers() {
        let first = export_migration_snapshot(&checkpoint()).unwrap();
        let mut reordered = checkpoint();
        reordered.ovl_balances.reverse();
        reordered.bridge.balances.reverse();
        let second = export_migration_snapshot(&reordered).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.body.audit.ovl.proposed_claims, 100);
        assert_eq!(first.body.audit.ovl.retired_or_burned, 10);
        assert_eq!(first.body.audit.drc.proposed_claims, 60);
        assert!(first.body.audit.ready_for_claim_design);
        verify_migration_snapshot(&first).unwrap();
    }

    #[test]
    fn district_balances_merge_by_address_without_losing_audit_rows() {
        let snapshot = export_migration_snapshot(&checkpoint()).unwrap();
        let alice = Address([1; 20]).to_hex();
        let liquid = snapshot
            .body
            .allocations
            .iter()
            .find(|allocation| {
                allocation.asset == MigrationAsset::Drc
                    && allocation.kind == AllocationKind::Liquid
                    && allocation.address == alice
            })
            .unwrap();
        assert_eq!(liquid.amount, 50);
        assert_eq!(snapshot.body.district_balances.len(), 3);
    }

    #[test]
    fn unresolved_locks_messages_and_evm_state_block_claim_design() {
        let mut source = checkpoint();
        source
            .bridge
            .locks
            .push(("hub".into(), Address([9; 20]), 5));
        let message = BridgeMessage {
            direction: BridgeDirection::Payment,
            source_district: "east".into(),
            dest_district: "east".into(),
            sender: Address([1; 20]),
            recipient: Address([2; 20]),
            amount: agora_types::Amount::from_base_units(2),
            nonce: 1,
            destination_tag: 0,
        };
        source
            .bridge
            .messages
            .push((message.id().to_hex(), message, MessageStatus::Paid));
        source.head_state_root = Hash([8; 32]).to_hex();
        source.revm_snapshots.push((
            source.head_state_root.clone(),
            vec![agora_ovolos_rollup::AccountSnapDto {
                address: [7; 20],
                balance: {
                    let mut value = [0; 32];
                    value[31] = 1;
                    value
                },
                nonce: 0,
                code_hash: [0; 32],
                code: Vec::new(),
                storage: Vec::new(),
            }],
        ));

        let snapshot = export_migration_snapshot(&source).unwrap();
        assert!(!snapshot.body.audit.ready_for_claim_design);
        assert_eq!(snapshot.body.audit.lock_total, 5);
        assert_eq!(snapshot.body.audit.pending_message_count, 1);
        assert_eq!(snapshot.body.audit.quarantined_evm_account_count, 1);
    }

    #[test]
    fn verify_detects_allocation_tampering() {
        let mut snapshot = export_migration_snapshot(&checkpoint()).unwrap();
        snapshot.body.allocations[0].amount += 1;
        assert!(matches!(
            verify_migration_snapshot(&snapshot),
            Err(MigrationError::RootMismatch { .. })
        ));
    }

    #[test]
    fn escrow_mismatch_is_reported_not_silently_allocated() {
        let mut source = checkpoint();
        source
            .ovl_balances
            .iter_mut()
            .find(|(address, _)| *address == SEQUENCER_BOND_ESCROW)
            .unwrap()
            .1 = 41;
        source.ovl_minted += 1;
        let snapshot = export_migration_snapshot(&source).unwrap();
        assert!(!snapshot.body.audit.ready_for_claim_design);
        assert!(snapshot
            .body
            .audit
            .blockers
            .iter()
            .any(|blocker| blocker.contains("escrow mismatch")));
    }
}
