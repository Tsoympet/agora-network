//! Canonical community registry for hubs, passports, grants, and missions.
//!
//! Records are append-only in v1. A compact summary keeps root reads O(1), while
//! the individual records remain available through deterministic prefix scans.

use agora_crypto::verify_passport_attestation_bound;
use agora_governance::{GrantRecord, HubAccreditationStatus, HubRecord, MissionRecord};
use agora_types::{Address, Amount, Hash, PassportAttestation};
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

fn issuer_is_active_hub_coordinator(
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
    let canonical = GrantRecord::new(
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
    if &canonical != grant {
        return Err(invalid(
            "new grant must satisfy constructor invariants without released funds",
        ));
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
    use agora_crypto::{sign_passport_attestation_bound, KeyPair};
    use agora_governance::{GrantKind, GrantStatus, MissionStatus};
    use agora_types::{NativeAssetId, PassportCategory, TreasuryId};

    use super::*;
    use crate::{load_issued_supply, load_protocol_treasuries, GenesisBuilder};

    fn auth() -> TxAuthContext {
        TxAuthContext {
            chain_id: "agora-community-test".into(),
            genesis: Hash([9; 32]),
        }
    }

    fn active_hub(id: u8, coordinator: Address) -> HubRecord {
        HubRecord {
            id: Hash([id; 32]),
            public_name: format!("Agora Hub {id}"),
            classification: "Geographic".into(),
            charter_hash: Hash([id.wrapping_add(1); 32]),
            coordinators: vec![coordinator],
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

        let grant = GrantRecord::new(
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
}
