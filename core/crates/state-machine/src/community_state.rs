//! Canonical community registry for hubs, passports, grants, and missions.
//!
//! Records are append-only in v1. A compact summary keeps root reads O(1), while
//! the individual records remain available through deterministic prefix scans.

use agora_crypto::{
    verify_grant_registration_bound, verify_hub_registration_bound,
    verify_mission_registration_bound, verify_passport_attestation_bound,
};
use agora_governance::{
    GrantKind, GrantMilestone, GrantRecord, HubAccreditationStatus, HubRecord, MilestoneStatus,
    MissionRecord, MissionStatus,
};
use agora_types::{
    Address, Amount, CommunityGrantKind, GrantRegistration, Hash, HubRegistration,
    MissionRegistration, PassportAttestation, TreasuryId,
};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::columns::ColumnFamily;
use crate::{StateError, StateStore, TxAuthContext, WriteBatch};

pub const CANONICAL_COMMUNITY_VERSION: u32 = 1;

const SUMMARY_KEY: &[u8] = b"community/v1/summary";
const HUB_PREFIX: &[u8] = b"community/v1/hub/";
const PASSPORT_PREFIX: &[u8] = b"community/v1/passport/";
const GRANT_PREFIX: &[u8] = b"community/v1/grant/";
const MISSION_PREFIX: &[u8] = b"community/v1/mission/";
const ISSUER_NONCE_PREFIX: &[u8] = b"community/v1/issuer_nonce/";
const HUB_NONCE_PREFIX: &[u8] = b"community/v1/hub_nonce/";
const GRANT_NONCE_PREFIX: &[u8] = b"community/v1/grant_nonce/";
const SPONSOR_NONCE_PREFIX: &[u8] = b"community/v1/sponsor_nonce/";
const ACTIVE_ISSUER_PREFIX: &[u8] = b"community/v1/active_issuer/";
const EMPTY_ROOT_DOMAIN: &[u8] = b"agora-community-empty-root-v1";
const ROLLING_ROOT_DOMAIN: &[u8] = b"agora-community-rolling-root-v1";

const HUB_KIND: &[u8] = b"hub";
const PASSPORT_KIND: &[u8] = b"passport";
const GRANT_KIND: &[u8] = b"grant";
const MISSION_KIND: &[u8] = b"mission";

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CanonicalCommunitySummary {
    pub version: u32,
    pub root: Hash,
    pub hub_count: u64,
    pub passport_count: u64,
    pub grant_count: u64,
    pub mission_count: u64,
}

impl Default for CanonicalCommunitySummary {
    fn default() -> Self {
        Self {
            version: CANONICAL_COMMUNITY_VERSION,
            root: Hash::hash_borsh(&(EMPTY_ROOT_DOMAIN, CANONICAL_COMMUNITY_VERSION)),
            hub_count: 0,
            passport_count: 0,
            grant_count: 0,
            mission_count: 0,
        }
    }
}

fn keyed(prefix: &[u8], id: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(prefix.len() + id.len());
    key.extend_from_slice(prefix);
    key.extend_from_slice(id);
    key
}

fn record_key(prefix: &[u8], id: &Hash) -> Vec<u8> {
    keyed(prefix, id.as_bytes())
}

fn issuer_nonce_key(issuer: &Address) -> Vec<u8> {
    keyed(ISSUER_NONCE_PREFIX, &issuer.0)
}

pub fn passport_record_key(id: &Hash) -> Vec<u8> {
    record_key(PASSPORT_PREFIX, id)
}

pub fn passport_issuer_nonce_key(issuer: &Address) -> Vec<u8> {
    issuer_nonce_key(issuer)
}

pub fn hub_record_key(id: &Hash) -> Vec<u8> {
    record_key(HUB_PREFIX, id)
}

pub fn grant_record_key(id: &Hash) -> Vec<u8> {
    record_key(GRANT_PREFIX, id)
}

pub fn mission_record_key(id: &Hash) -> Vec<u8> {
    record_key(MISSION_PREFIX, id)
}

pub fn hub_coordinator_nonce_key(coordinator: &Address) -> Vec<u8> {
    keyed(HUB_NONCE_PREFIX, &coordinator.0)
}

pub fn grant_registrar_nonce_key(registrar: &Address) -> Vec<u8> {
    keyed(GRANT_NONCE_PREFIX, &registrar.0)
}

pub fn mission_sponsor_nonce_key(sponsor: &Address) -> Vec<u8> {
    keyed(SPONSOR_NONCE_PREFIX, &sponsor.0)
}

pub fn active_hub_coordinator_key(coordinator: &Address) -> Vec<u8> {
    keyed(ACTIVE_ISSUER_PREFIX, &coordinator.0)
}

pub fn community_summary_key() -> Vec<u8> {
    SUMMARY_KEY.to_vec()
}

pub fn load_passport_attestation(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<PassportAttestation>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &passport_record_key(id))? else {
        return Ok(None);
    };
    Ok(Some(decode(&bytes)?))
}

pub fn load_passport_issuer_nonce(store: &StateStore, issuer: &Address) -> Result<u64, StateError> {
    load_issuer_nonce(store, issuer)
}

pub fn load_hub_registration(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<HubRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &hub_record_key(id))? else {
        return Ok(None);
    };
    Ok(Some(decode(&bytes)?))
}

pub fn load_grant_registration(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<GrantRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &grant_record_key(id))? else {
        return Ok(None);
    };
    Ok(Some(decode(&bytes)?))
}

pub fn load_mission_registration(
    store: &StateStore,
    id: &Hash,
) -> Result<Option<MissionRecord>, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &mission_record_key(id))? else {
        return Ok(None);
    };
    Ok(Some(decode(&bytes)?))
}

fn load_prefixed_nonce(store: &StateStore, key: &[u8]) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, key)? else {
        return Ok(0);
    };
    decode(&bytes)
}

pub fn load_hub_coordinator_nonce(
    store: &StateStore,
    coordinator: &Address,
) -> Result<u64, StateError> {
    load_prefixed_nonce(store, &hub_coordinator_nonce_key(coordinator))
}

pub fn load_grant_registrar_nonce(
    store: &StateStore,
    registrar: &Address,
) -> Result<u64, StateError> {
    load_prefixed_nonce(store, &grant_registrar_nonce_key(registrar))
}

pub fn load_mission_sponsor_nonce(
    store: &StateStore,
    sponsor: &Address,
) -> Result<u64, StateError> {
    load_prefixed_nonce(store, &mission_sponsor_nonce_key(sponsor))
}

fn active_issuer_key(issuer: &Address) -> Vec<u8> {
    keyed(ACTIVE_ISSUER_PREFIX, &issuer.0)
}

fn invalid(message: impl Into<String>) -> StateError {
    StateError::InvalidTx(message.into())
}

fn encode<T: BorshSerialize>(value: &T) -> Result<Vec<u8>, StateError> {
    borsh::to_vec(value).map_err(|error| StateError::Storage(error.to_string()))
}

fn decode<T: BorshDeserialize>(bytes: &[u8]) -> Result<T, StateError> {
    T::try_from_slice(bytes).map_err(|error| StateError::Storage(error.to_string()))
}

fn ensure_absent(store: &StateStore, key: &[u8], kind: &str) -> Result<(), StateError> {
    if store.get_cf(ColumnFamily::Meta, key)?.is_some() {
        return Err(invalid(format!("duplicate canonical community {kind}")));
    }
    Ok(())
}

fn updated_root<T: BorshSerialize>(prior_root: Hash, kind: &[u8], id: Hash, record: &T) -> Hash {
    let record_hash = Hash::hash_borsh(record);
    Hash::hash_borsh(&(ROLLING_ROOT_DOMAIN, prior_root, kind, id, record_hash))
}

fn put_summary_into(
    batch: &mut WriteBatch,
    summary: &CanonicalCommunitySummary,
) -> Result<(), StateError> {
    batch.put_cf(ColumnFamily::Meta, SUMMARY_KEY, &encode(summary)?);
    Ok(())
}

pub fn init_canonical_community_into(batch: &mut WriteBatch) -> Result<(), StateError> {
    let mut pending = WriteBatch::new();
    put_summary_into(&mut pending, &CanonicalCommunitySummary::default())?;
    batch.append(pending);
    Ok(())
}

pub fn load_canonical_community_summary(
    store: &StateStore,
) -> Result<CanonicalCommunitySummary, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, SUMMARY_KEY)? else {
        return Ok(CanonicalCommunitySummary::default());
    };
    let summary = decode::<CanonicalCommunitySummary>(&bytes)?;
    if summary.version != CANONICAL_COMMUNITY_VERSION {
        return Err(StateError::Storage(
            "unsupported canonical community summary version".into(),
        ));
    }
    Ok(summary)
}

pub fn canonical_community_root(store: &StateStore) -> Result<Hash, StateError> {
    Ok(load_canonical_community_summary(store)?.root)
}

fn list_records<T: BorshDeserialize>(
    store: &StateStore,
    prefix: &[u8],
    limit: usize,
) -> Result<Vec<T>, StateError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    store
        .scan_prefix(ColumnFamily::Meta, prefix)?
        .into_iter()
        .take(limit)
        .map(|(_, bytes)| decode(&bytes))
        .collect()
}

pub fn list_hubs(store: &StateStore, limit: usize) -> Result<Vec<HubRecord>, StateError> {
    list_records(store, HUB_PREFIX, limit)
}

pub fn list_passport_attestations(
    store: &StateStore,
    limit: usize,
) -> Result<Vec<PassportAttestation>, StateError> {
    list_records(store, PASSPORT_PREFIX, limit)
}

pub fn list_grants(store: &StateStore, limit: usize) -> Result<Vec<GrantRecord>, StateError> {
    list_records(store, GRANT_PREFIX, limit)
}

pub fn list_missions(store: &StateStore, limit: usize) -> Result<Vec<MissionRecord>, StateError> {
    list_records(store, MISSION_PREFIX, limit)
}

pub fn register_hub_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    hub: &HubRecord,
) -> Result<(), StateError> {
    if hub.id == Hash::ZERO {
        return Err(invalid("hub id must be nonzero"));
    }
    if hub.charter_hash == Hash::ZERO {
        return Err(invalid("hub charter hash must be nonzero"));
    }
    if hub.public_name.trim().is_empty() {
        return Err(invalid("hub public name must be nonempty"));
    }
    if hub.classification.trim().is_empty() {
        return Err(invalid("hub classification must be nonempty"));
    }
    if hub.coordinators.is_empty() {
        return Err(invalid("hub coordinators must be nonempty"));
    }
    if hub.coordinators.contains(&Address::ZERO) {
        return Err(invalid("hub coordinator must be nonzero"));
    }
    let unique: std::collections::HashSet<_> = hub.coordinators.iter().collect();
    if unique.len() != hub.coordinators.len() {
        return Err(invalid("hub coordinators must be unique"));
    }
    if hub.status == HubAccreditationStatus::Active && hub.accreditation_proposal_id == 0 {
        return Err(invalid(
            "active hub requires a canonical accreditation proposal",
        ));
    }
    if hub.status == HubAccreditationStatus::Active
        && (hub.treasury_multisig == Address::ZERO
            || hub.election_term_epochs == 0
            || hub.reporting_interval_epochs == 0
            || hub.coi_disclosure_root == Hash::ZERO)
    {
        return Err(invalid(
            "active hub requires multisig, terms, reporting, and COI commitment",
        ));
    }

    let key = record_key(HUB_PREFIX, &hub.id);
    ensure_absent(store, &key, "hub")?;
    let mut summary = load_canonical_community_summary(store)?;
    summary.hub_count = summary
        .hub_count
        .checked_add(1)
        .ok_or_else(|| invalid("canonical community hub count overflow"))?;
    summary.root = updated_root(summary.root, HUB_KIND, hub.id, hub);

    let mut pending = WriteBatch::new();
    pending.put_cf(ColumnFamily::Meta, &key, &encode(hub)?);
    if hub.status == HubAccreditationStatus::Active {
        for coordinator in &hub.coordinators {
            pending.put_cf(
                ColumnFamily::Meta,
                &active_issuer_key(coordinator),
                hub.id.as_bytes(),
            );
        }
    }
    put_summary_into(&mut pending, &summary)?;
    batch.append(pending);
    Ok(())
}

fn load_issuer_nonce(store: &StateStore, issuer: &Address) -> Result<u64, StateError> {
    let Some(bytes) = store.get_cf(ColumnFamily::Meta, &issuer_nonce_key(issuer))? else {
        return Ok(0);
    };
    decode(&bytes)
}

pub fn issuer_is_active_hub_coordinator(
    store: &StateStore,
    issuer: &Address,
) -> Result<bool, StateError> {
    Ok(store
        .get_cf(ColumnFamily::Meta, &active_issuer_key(issuer))?
        .is_some())
}

pub fn register_passport_attestation_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    attestation: &PassportAttestation,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    if attestation.version != 1 {
        return Err(invalid("unsupported passport attestation version"));
    }
    if attestation.subject == Address::ZERO {
        return Err(invalid("passport subject must be nonzero"));
    }
    if attestation.evidence_hash == Hash::ZERO {
        return Err(invalid("passport evidence hash must be nonzero"));
    }
    if attestation.issuer_policy_hash == Hash::ZERO {
        return Err(invalid("passport issuer policy hash must be nonzero"));
    }
    if attestation
        .expires_epoch
        .is_some_and(|expiry| expiry <= attestation.issued_epoch)
    {
        return Err(invalid(
            "passport expiry must be later than its issued epoch",
        ));
    }
    verify_passport_attestation_bound(attestation, &auth.chain_id, &auth.genesis)
        .map_err(|error| invalid(error.to_string()))?;

    let id = attestation.attestation_id();
    let key = record_key(PASSPORT_PREFIX, &id);
    ensure_absent(store, &key, "passport attestation")?;
    let current_nonce = load_issuer_nonce(store, &attestation.issuer)?;
    if attestation.nonce != current_nonce {
        return Err(invalid("passport issuer nonce mismatch"));
    }
    if !issuer_is_active_hub_coordinator(store, &attestation.issuer)? {
        return Err(invalid(
            "passport issuer is not an active canonical hub coordinator",
        ));
    }

    let next_nonce = current_nonce
        .checked_add(1)
        .ok_or_else(|| invalid("passport issuer nonce overflow"))?;
    let mut summary = load_canonical_community_summary(store)?;
    summary.passport_count = summary
        .passport_count
        .checked_add(1)
        .ok_or_else(|| invalid("canonical community passport count overflow"))?;
    summary.root = updated_root(summary.root, PASSPORT_KIND, id, attestation);

    let mut pending = WriteBatch::new();
    pending.put_cf(ColumnFamily::Meta, &key, &encode(attestation)?);
    pending.put_cf(
        ColumnFamily::Meta,
        &issuer_nonce_key(&attestation.issuer),
        &encode(&next_nonce)?,
    );
    put_summary_into(&mut pending, &summary)?;
    batch.append(pending);
    Ok(())
}

fn map_grant_kind(kind: CommunityGrantKind) -> GrantKind {
    match kind {
        CommunityGrantKind::Micro => GrantKind::Micro,
        CommunityGrantKind::Milestone => GrantKind::Milestone,
        CommunityGrantKind::Bounty => GrantKind::Bounty,
        CommunityGrantKind::Retroactive => GrantKind::Retroactive,
    }
}

fn hub_record_from_registration(registration: &HubRegistration) -> Result<HubRecord, StateError> {
    Ok(HubRecord {
        id: registration.registration_id(),
        public_name: registration.public_name.clone(),
        classification: registration.classification.clone(),
        charter_hash: registration.charter_hash,
        coordinators: registration.coordinators.clone(),
        treasury_multisig: registration.treasury_multisig,
        election_term_epochs: registration.election_term_epochs,
        reporting_interval_epochs: registration.reporting_interval_epochs,
        coi_disclosure_root: registration.coi_disclosure_root,
        deliverables_root: registration.deliverables_root,
        accreditation_proposal_id: registration.accreditation_proposal_id,
        status: HubAccreditationStatus::Active,
    })
}

fn grant_record_from_registration(
    registration: &GrantRegistration,
) -> Result<GrantRecord, StateError> {
    let milestones = registration
        .milestones
        .iter()
        .map(|milestone| GrantMilestone {
            index: milestone.index,
            amount: milestone.amount,
            deliverable_hash: milestone.deliverable_hash,
            status: MilestoneStatus::Pending,
        })
        .collect();
    let mut grant = GrantRecord::new(
        registration.registration_id(),
        registration.proposal_id,
        registration.treasury,
        registration.beneficiary,
        registration.total,
        map_grant_kind(registration.kind),
        agora_governance::GrantStatus::Approved,
        milestones,
    )
    .map_err(|error| invalid(error.to_string()))?;
    if registration.treasury == TreasuryId::DrcCommunity {
        grant
            .record_conflict_review(true, registration.coi_disclosure_hash)
            .map_err(|error| invalid(error.to_string()))?;
    } else if registration.coi_disclosure_hash != Hash::ZERO {
        return Err(invalid("non-community grant must not carry COI disclosure"));
    }
    Ok(grant)
}

fn mission_record_from_registration(
    registration: &MissionRegistration,
) -> Result<MissionRecord, StateError> {
    Ok(MissionRecord {
        id: registration.registration_id(),
        sponsor: registration.sponsor,
        reward_treasury: registration.reward_treasury,
        reward: registration.reward,
        requirements_hash: registration.requirements_hash,
        assignee: None,
        status: MissionStatus::Open,
        completion_evidence: Hash::ZERO,
    })
}

pub fn register_signed_hub_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    registration: &HubRegistration,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    if registration.version != 1 {
        return Err(invalid("unsupported hub registration version"));
    }
    verify_hub_registration_bound(registration, &auth.chain_id, &auth.genesis)
        .map_err(|error| invalid(error.to_string()))?;
    let coordinator = registration
        .first_coordinator()
        .ok_or_else(|| invalid("hub coordinators must be nonempty"))?;
    let current_nonce = load_hub_coordinator_nonce(store, &coordinator)?;
    if registration.nonce != current_nonce {
        return Err(invalid("hub coordinator nonce mismatch"));
    }
    let next_nonce = current_nonce
        .checked_add(1)
        .ok_or_else(|| invalid("hub coordinator nonce overflow"))?;
    let hub = hub_record_from_registration(registration)?;
    register_hub_into(batch, store, &hub)?;
    let mut pending = WriteBatch::new();
    pending.put_cf(
        ColumnFamily::Meta,
        &hub_coordinator_nonce_key(&coordinator),
        &encode(&next_nonce)?,
    );
    batch.append(pending);
    Ok(())
}

pub fn register_signed_grant_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    registration: &GrantRegistration,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    if registration.version != 1 {
        return Err(invalid("unsupported grant registration version"));
    }
    verify_grant_registration_bound(registration, &auth.chain_id, &auth.genesis)
        .map_err(|error| invalid(error.to_string()))?;
    if !issuer_is_active_hub_coordinator(store, &registration.registrar)? {
        return Err(invalid(
            "grant registrar is not an active canonical hub coordinator",
        ));
    }
    let current_nonce = load_grant_registrar_nonce(store, &registration.registrar)?;
    if registration.nonce != current_nonce {
        return Err(invalid("grant registrar nonce mismatch"));
    }
    let next_nonce = current_nonce
        .checked_add(1)
        .ok_or_else(|| invalid("grant registrar nonce overflow"))?;
    let grant = grant_record_from_registration(registration)?;
    register_grant_into(batch, store, &grant)?;
    let mut pending = WriteBatch::new();
    pending.put_cf(
        ColumnFamily::Meta,
        &grant_registrar_nonce_key(&registration.registrar),
        &encode(&next_nonce)?,
    );
    batch.append(pending);
    Ok(())
}

pub fn register_signed_mission_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    registration: &MissionRegistration,
    auth: &TxAuthContext,
) -> Result<(), StateError> {
    if registration.version != 1 {
        return Err(invalid("unsupported mission registration version"));
    }
    verify_mission_registration_bound(registration, &auth.chain_id, &auth.genesis)
        .map_err(|error| invalid(error.to_string()))?;
    if !issuer_is_active_hub_coordinator(store, &registration.sponsor)? {
        return Err(invalid(
            "mission sponsor is not an active canonical hub coordinator",
        ));
    }
    let current_nonce = load_mission_sponsor_nonce(store, &registration.sponsor)?;
    if registration.nonce != current_nonce {
        return Err(invalid("mission sponsor nonce mismatch"));
    }
    let next_nonce = current_nonce
        .checked_add(1)
        .ok_or_else(|| invalid("mission sponsor nonce overflow"))?;
    let mission = mission_record_from_registration(registration)?;
    register_mission_into(batch, store, &mission)?;
    let mut pending = WriteBatch::new();
    pending.put_cf(
        ColumnFamily::Meta,
        &mission_sponsor_nonce_key(&registration.sponsor),
        &encode(&next_nonce)?,
    );
    batch.append(pending);
    Ok(())
}

pub fn register_grant_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    grant: &GrantRecord,
) -> Result<(), StateError> {
    if grant.id == Hash::ZERO {
        return Err(invalid("grant id must be nonzero"));
    }
    if grant.proposal_id == 0 {
        return Err(invalid("grant proposal id must be nonzero"));
    }
    if grant.status != agora_governance::GrantStatus::Approved
        || grant.released != Amount::ZERO
        || grant
            .milestones
            .iter()
            .any(|m| m.status != agora_governance::MilestoneStatus::Pending)
    {
        return Err(invalid(
            "grant registration requires pristine approved state",
        ));
    }
    let _validated_shape = GrantRecord::new(
        grant.id,
        grant.proposal_id,
        grant.treasury,
        grant.beneficiary,
        grant.total,
        grant.kind,
        grant.status,
        grant.milestones.clone(),
    )
    .map_err(|error| invalid(error.to_string()))?;
    if grant.treasury == TreasuryId::DrcCommunity
        && (grant.conflict_review != agora_governance::ConflictReviewStatus::Cleared
            || grant.coi_disclosure_hash == Hash::ZERO)
    {
        return Err(invalid("DRC community grant requires cleared COI evidence"));
    }
    if grant.treasury != TreasuryId::DrcCommunity
        && grant.conflict_review != agora_governance::ConflictReviewStatus::NotRequired
    {
        return Err(invalid("non-community grant has invalid COI state"));
    }

    let key = record_key(GRANT_PREFIX, &grant.id);
    ensure_absent(store, &key, "grant")?;
    let mut summary = load_canonical_community_summary(store)?;
    summary.grant_count = summary
        .grant_count
        .checked_add(1)
        .ok_or_else(|| invalid("canonical community grant count overflow"))?;
    summary.root = updated_root(summary.root, GRANT_KIND, grant.id, grant);

    let mut pending = WriteBatch::new();
    pending.put_cf(ColumnFamily::Meta, &key, &encode(grant)?);
    put_summary_into(&mut pending, &summary)?;
    batch.append(pending);
    Ok(())
}

pub fn register_mission_into(
    batch: &mut WriteBatch,
    store: &StateStore,
    mission: &MissionRecord,
) -> Result<(), StateError> {
    if mission.id == Hash::ZERO {
        return Err(invalid("mission id must be nonzero"));
    }
    if mission.reward == Amount::ZERO {
        return Err(invalid("mission reward must be nonzero"));
    }
    if mission.requirements_hash == Hash::ZERO {
        return Err(invalid("mission requirements hash must be nonzero"));
    }
    if mission.status != agora_governance::MissionStatus::Open
        || mission.assignee.is_some()
        || mission.completion_evidence != Hash::ZERO
    {
        return Err(invalid("mission registration requires pristine open state"));
    }

    let key = record_key(MISSION_PREFIX, &mission.id);
    ensure_absent(store, &key, "mission")?;
    let mut summary = load_canonical_community_summary(store)?;
    summary.mission_count = summary
        .mission_count
        .checked_add(1)
        .ok_or_else(|| invalid("canonical community mission count overflow"))?;
    summary.root = updated_root(summary.root, MISSION_KIND, mission.id, mission);

    let mut pending = WriteBatch::new();
    pending.put_cf(ColumnFamily::Meta, &key, &encode(mission)?);
    put_summary_into(&mut pending, &summary)?;
    batch.append(pending);
    Ok(())
}

#[cfg(test)]
mod tests {
    use agora_crypto::{
        sign_grant_registration_bound, sign_hub_registration_bound,
        sign_mission_registration_bound, sign_passport_attestation_bound, KeyPair,
    };
    use agora_governance::{GrantKind, GrantStatus, MissionStatus};
    use agora_types::{
        CommunityGrantKind, GrantRegistration, HubRegistration, MissionRegistration, NativeAssetId,
        PassportCategory, TreasuryId,
    };

    use super::*;
    use crate::{load_issued_supply, load_protocol_treasuries, GenesisBuilder};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-community-test".into(),
            genesis: Hash([9; 32]),
            data_availability_network_fingerprint: None,
        }
    }

    fn active_hub(id: u8, coordinator: Address) -> HubRecord {
        HubRecord {
            id: Hash([id; 32]),
            public_name: format!("Agora Hub {id}"),
            classification: "Geographic".into(),
            charter_hash: Hash([id.wrapping_add(1); 32]),
            coordinators: vec![coordinator],
            treasury_multisig: Address([id.wrapping_add(2); 20]),
            election_term_epochs: 12,
            reporting_interval_epochs: 3,
            coi_disclosure_root: Hash([id.wrapping_add(3); 32]),
            deliverables_root: Hash([id.wrapping_add(4); 32]),
            accreditation_proposal_id: 1,
            status: HubAccreditationStatus::Active,
        }
    }

    fn signed_passport(issuer: &KeyPair, nonce: u64, auth: &TxAuthContext) -> PassportAttestation {
        let mut attestation = PassportAttestation::unsigned(
            issuer.address(),
            Address([7; 20]),
            PassportCategory::CommunitySupport,
            Hash([3; 32]),
            Hash([4; 32]),
            10,
            Some(20),
            nonce,
        );
        sign_passport_attestation_bound(&mut attestation, issuer, &auth.chain_id, &auth.genesis)
            .unwrap();
        attestation
    }

    #[test]
    fn empty_genesis_summary_and_root_are_stable() {
        let missing = StateStore::open_in_memory();
        let expected = CanonicalCommunitySummary::default();
        assert_eq!(borsh::to_vec(&expected).unwrap().len(), 68);
        assert_eq!(
            load_canonical_community_summary(&missing).unwrap(),
            expected
        );

        let store = StateStore::open_in_memory();
        GenesisBuilder::default().ignite(&store).unwrap();
        assert_eq!(load_canonical_community_summary(&store).unwrap(), expected);
        assert_eq!(canonical_community_root(&store).unwrap(), expected.root);
        assert_eq!(canonical_community_root(&missing).unwrap(), expected.root);
    }

    #[test]
    fn active_hub_then_signed_passport_are_accepted_and_listed() {
        let store = StateStore::open_in_memory();
        let issuer = KeyPair::from_secret_bytes(&[1; 32]).unwrap();
        let auth = auth();
        let hub = active_hub(1, issuer.address());

        let initial_root = canonical_community_root(&store).unwrap();
        let mut hub_batch = WriteBatch::new();
        register_hub_into(&mut hub_batch, &store, &hub).unwrap();
        store.write_batch(hub_batch).unwrap();
        let hub_root = canonical_community_root(&store).unwrap();
        assert_ne!(hub_root, initial_root);

        let attestation = signed_passport(&issuer, 0, &auth);
        let mut passport_batch = WriteBatch::new();
        register_passport_attestation_into(&mut passport_batch, &store, &attestation, &auth)
            .unwrap();
        store.write_batch(passport_batch).unwrap();

        assert_ne!(canonical_community_root(&store).unwrap(), hub_root);
        assert_eq!(list_hubs(&store, 10).unwrap(), vec![hub]);
        assert_eq!(
            list_passport_attestations(&store, 10).unwrap(),
            vec![attestation]
        );
        let summary = load_canonical_community_summary(&store).unwrap();
        assert_eq!(summary.hub_count, 1);
        assert_eq!(summary.passport_count, 1);
    }

    #[test]
    fn non_hub_issuer_is_rejected_without_writes() {
        let store = StateStore::open_in_memory();
        let issuer = KeyPair::from_secret_bytes(&[2; 32]).unwrap();
        let auth = auth();
        let attestation = signed_passport(&issuer, 0, &auth);
        let mut batch = WriteBatch::new();

        assert!(
            register_passport_attestation_into(&mut batch, &store, &attestation, &auth).is_err()
        );
        assert!(batch.is_empty());
        assert_eq!(
            load_canonical_community_summary(&store).unwrap(),
            CanonicalCommunitySummary::default()
        );
    }

    #[test]
    fn duplicate_and_nonce_rejections_append_no_writes() {
        let store = StateStore::open_in_memory();
        let issuer = KeyPair::from_secret_bytes(&[3; 32]).unwrap();
        let auth = auth();
        let hub = active_hub(3, issuer.address());
        let mut batch = WriteBatch::new();
        register_hub_into(&mut batch, &store, &hub).unwrap();
        store.write_batch(batch).unwrap();

        let mut duplicate_hub_batch = WriteBatch::new();
        assert!(register_hub_into(&mut duplicate_hub_batch, &store, &hub).is_err());
        assert!(duplicate_hub_batch.is_empty());

        let bad_nonce = signed_passport(&issuer, 1, &auth);
        let mut nonce_batch = WriteBatch::new();
        assert!(
            register_passport_attestation_into(&mut nonce_batch, &store, &bad_nonce, &auth)
                .is_err()
        );
        assert!(nonce_batch.is_empty());

        let accepted = signed_passport(&issuer, 0, &auth);
        let mut accepted_batch = WriteBatch::new();
        register_passport_attestation_into(&mut accepted_batch, &store, &accepted, &auth).unwrap();
        store.write_batch(accepted_batch).unwrap();

        let mut duplicate_batch = WriteBatch::new();
        assert!(
            register_passport_attestation_into(&mut duplicate_batch, &store, &accepted, &auth)
                .is_err()
        );
        assert!(duplicate_batch.is_empty());
    }

    #[test]
    fn grants_and_missions_list_without_monetary_side_effects() {
        let store = StateStore::open_in_memory();
        GenesisBuilder::default().ignite(&store).unwrap();
        let supplies_before = [
            load_issued_supply(&store, NativeAssetId::TLT).unwrap(),
            load_issued_supply(&store, NativeAssetId::OVL).unwrap(),
            load_issued_supply(&store, NativeAssetId::DRC).unwrap(),
        ];
        let treasuries_before = load_protocol_treasuries(&store).unwrap();

        let mut grant = GrantRecord::new(
            Hash([5; 32]),
            7,
            TreasuryId::DrcCommunity,
            Address([6; 20]),
            Amount::from_base_units(100),
            GrantKind::Micro,
            GrantStatus::Approved,
            vec![],
        )
        .unwrap();
        grant.record_conflict_review(true, Hash([11; 32])).unwrap();
        let mission = MissionRecord {
            id: Hash([8; 32]),
            sponsor: Address([9; 20]),
            reward_treasury: TreasuryId::OvlBuilder,
            reward: Amount::from_base_units(50),
            requirements_hash: Hash([10; 32]),
            assignee: None,
            status: MissionStatus::Open,
            completion_evidence: Hash::ZERO,
        };

        let initial_root = canonical_community_root(&store).unwrap();
        let mut grant_batch = WriteBatch::new();
        register_grant_into(&mut grant_batch, &store, &grant).unwrap();
        store.write_batch(grant_batch).unwrap();
        let grant_root = canonical_community_root(&store).unwrap();
        assert_ne!(grant_root, initial_root);

        let mut mission_batch = WriteBatch::new();
        register_mission_into(&mut mission_batch, &store, &mission).unwrap();
        store.write_batch(mission_batch).unwrap();
        assert_ne!(canonical_community_root(&store).unwrap(), grant_root);

        assert_eq!(list_grants(&store, 1).unwrap(), vec![grant]);
        assert_eq!(list_missions(&store, 1).unwrap(), vec![mission]);
        assert!(list_grants(&store, 0).unwrap().is_empty());
        assert_eq!(
            [
                load_issued_supply(&store, NativeAssetId::TLT).unwrap(),
                load_issued_supply(&store, NativeAssetId::OVL).unwrap(),
                load_issued_supply(&store, NativeAssetId::DRC).unwrap(),
            ],
            supplies_before
        );
        assert_eq!(load_protocol_treasuries(&store).unwrap(), treasuries_before);
    }

    #[test]
    fn canonical_registration_rejects_non_pristine_grant_and_mission() {
        let store = StateStore::open_in_memory();
        let mut grant = GrantRecord::new(
            Hash([11; 32]),
            1,
            TreasuryId::OvlBuilder,
            Address([12; 20]),
            Amount::from_base_units(10),
            GrantKind::Micro,
            GrantStatus::Approved,
            vec![],
        )
        .unwrap();
        grant.status = GrantStatus::Active;
        let mut batch = WriteBatch::new();
        assert!(register_grant_into(&mut batch, &store, &grant).is_err());
        assert!(batch.is_empty());

        let mission = MissionRecord {
            id: Hash([13; 32]),
            sponsor: Address([14; 20]),
            reward_treasury: TreasuryId::DrcCommunity,
            reward: Amount::from_base_units(5),
            requirements_hash: Hash([15; 32]),
            assignee: Some(Address([16; 20])),
            status: MissionStatus::Assigned,
            completion_evidence: Hash::ZERO,
        };
        assert!(register_mission_into(&mut batch, &store, &mission).is_err());
        assert!(batch.is_empty());
    }

    fn signed_hub(coordinator: &KeyPair, nonce: u64, auth: &TxAuthContext) -> HubRegistration {
        let mut registration = HubRegistration::unsigned(
            "Agora Signed Hub".into(),
            "Geographic".into(),
            Hash([2; 32]),
            vec![coordinator.address()],
            Address([3; 20]),
            12,
            3,
            Hash([4; 32]),
            Hash([5; 32]),
            1,
            nonce,
        );
        sign_hub_registration_bound(
            &mut registration,
            coordinator,
            &auth.chain_id,
            &auth.genesis,
        )
        .unwrap();
        registration
    }

    #[test]
    fn signed_hub_then_grant_and_mission_are_accepted() {
        let store = StateStore::open_in_memory();
        let coordinator = KeyPair::from_secret_bytes(&[4; 32]).unwrap();
        let auth = auth();
        let hub = signed_hub(&coordinator, 0, &auth);
        let mut hub_batch = WriteBatch::new();
        register_signed_hub_into(&mut hub_batch, &store, &hub, &auth).unwrap();
        store.write_batch(hub_batch).unwrap();
        assert_eq!(
            load_hub_coordinator_nonce(&store, &coordinator.address()).unwrap(),
            1
        );
        assert!(issuer_is_active_hub_coordinator(&store, &coordinator.address()).unwrap());

        let mut grant = GrantRegistration::unsigned(
            coordinator.address(),
            7,
            TreasuryId::OvlBuilder,
            Address([6; 20]),
            Amount::from_base_units(100),
            CommunityGrantKind::Micro,
            vec![],
            Hash::ZERO,
            0,
        );
        sign_grant_registration_bound(&mut grant, &coordinator, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut grant_batch = WriteBatch::new();
        register_signed_grant_into(&mut grant_batch, &store, &grant, &auth).unwrap();
        store.write_batch(grant_batch).unwrap();

        let mut mission = MissionRegistration::unsigned(
            coordinator.address(),
            TreasuryId::OvlBuilder,
            Amount::from_base_units(50),
            Hash([10; 32]),
            0,
        );
        sign_mission_registration_bound(&mut mission, &coordinator, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut mission_batch = WriteBatch::new();
        register_signed_mission_into(&mut mission_batch, &store, &mission, &auth).unwrap();
        store.write_batch(mission_batch).unwrap();

        assert_eq!(list_grants(&store, 10).unwrap().len(), 1);
        assert_eq!(list_missions(&store, 10).unwrap().len(), 1);
        assert_eq!(
            load_grant_registrar_nonce(&store, &coordinator.address()).unwrap(),
            1
        );
        assert_eq!(
            load_mission_sponsor_nonce(&store, &coordinator.address()).unwrap(),
            1
        );

        let mut replay = WriteBatch::new();
        assert!(register_signed_hub_into(&mut replay, &store, &hub, &auth).is_err());
        assert!(replay.is_empty());
    }

    #[test]
    fn signed_grant_rejects_non_hub_registrar() {
        let store = StateStore::open_in_memory();
        let registrar = KeyPair::from_secret_bytes(&[5; 32]).unwrap();
        let auth = auth();
        let mut grant = GrantRegistration::unsigned(
            registrar.address(),
            1,
            TreasuryId::OvlBuilder,
            Address([6; 20]),
            Amount::from_base_units(10),
            CommunityGrantKind::Micro,
            vec![],
            Hash::ZERO,
            0,
        );
        sign_grant_registration_bound(&mut grant, &registrar, &auth.chain_id, &auth.genesis)
            .unwrap();
        let mut batch = WriteBatch::new();
        assert!(register_signed_grant_into(&mut batch, &store, &grant, &auth).is_err());
        assert!(batch.is_empty());
    }
}
