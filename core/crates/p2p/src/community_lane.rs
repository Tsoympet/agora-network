//! Mempool admission for signed Hub / Grant / Mission registrations.

use agora_types::{GrantRegistration, Hash, HubRegistration, MissionRegistration};

use crate::P2pError;

use super::Mempool;

impl Mempool {
    pub fn get_hub_registration(&self, id: &Hash) -> Option<&HubRegistration> {
        self.hub_registrations.get(id)
    }

    pub fn get_grant_registration(&self, id: &Hash) -> Option<&GrantRegistration> {
        self.grant_registrations.get(id)
    }

    pub fn get_mission_registration(&self, id: &Hash) -> Option<&MissionRegistration> {
        self.mission_registrations.get(id)
    }

    pub fn admit_hub_registration(
        &mut self,
        registration: HubRegistration,
    ) -> Result<Hash, P2pError> {
        if registration.version != 1 {
            return Err(P2pError::MempoolRejected(
                "unsupported hub registration version".into(),
            ));
        }
        if registration.public_key.len() != 33 || registration.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "hub registration missing secp256k1 auth".into(),
            ));
        }
        let coordinator = registration
            .first_coordinator()
            .ok_or_else(|| P2pError::MempoolRejected("hub coordinators must be nonempty".into()))?;
        let id = registration.registration_id();
        if self.hub_registrations.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self.reserved_hub_coordinators.contains_key(&coordinator) {
            return Err(P2pError::MempoolRejected(
                "coordinator already has a pending hub registration nonce".into(),
            ));
        }
        self.reserved_hub_coordinators.insert(coordinator, id);
        self.hub_registrations.insert(id, registration);
        Ok(id)
    }

    pub fn admit_grant_registration(
        &mut self,
        registration: GrantRegistration,
    ) -> Result<Hash, P2pError> {
        if registration.version != 1 {
            return Err(P2pError::MempoolRejected(
                "unsupported grant registration version".into(),
            ));
        }
        if registration.public_key.len() != 33 || registration.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "grant registration missing secp256k1 auth".into(),
            ));
        }
        let id = registration.registration_id();
        if self.grant_registrations.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_grant_registrars
            .contains_key(&registration.registrar)
        {
            return Err(P2pError::MempoolRejected(
                "registrar already has a pending grant registration nonce".into(),
            ));
        }
        self.reserved_grant_registrars
            .insert(registration.registrar, id);
        self.grant_registrations.insert(id, registration);
        Ok(id)
    }

    pub fn admit_mission_registration(
        &mut self,
        registration: MissionRegistration,
    ) -> Result<Hash, P2pError> {
        if registration.version != 1 {
            return Err(P2pError::MempoolRejected(
                "unsupported mission registration version".into(),
            ));
        }
        if registration.public_key.len() != 33 || registration.signature.len() != 64 {
            return Err(P2pError::MempoolRejected(
                "mission registration missing secp256k1 auth".into(),
            ));
        }
        let id = registration.registration_id();
        if self.mission_registrations.contains_key(&id) {
            return Ok(id);
        }
        if self.len() >= self.max_size {
            return Err(P2pError::MempoolRejected("mempool full".into()));
        }
        if self
            .reserved_mission_sponsors
            .contains_key(&registration.sponsor)
        {
            return Err(P2pError::MempoolRejected(
                "sponsor already has a pending mission registration nonce".into(),
            ));
        }
        self.reserved_mission_sponsors
            .insert(registration.sponsor, id);
        self.mission_registrations.insert(id, registration);
        Ok(id)
    }

    pub fn select_hub_registrations(&self, max: usize) -> Vec<HubRegistration> {
        select_sorted(
            &self.hub_registrations,
            max,
            HubRegistration::registration_id,
        )
    }

    pub fn select_grant_registrations(&self, max: usize) -> Vec<GrantRegistration> {
        select_sorted(
            &self.grant_registrations,
            max,
            GrantRegistration::registration_id,
        )
    }

    pub fn select_mission_registrations(&self, max: usize) -> Vec<MissionRegistration> {
        select_sorted(
            &self.mission_registrations,
            max,
            MissionRegistration::registration_id,
        )
    }

    pub(super) fn evict_community_registrations_from_block(&mut self, block: &agora_types::Block) {
        let mut hubs = std::collections::HashSet::new();
        for registration in &block.hub_registrations {
            self.hub_registrations
                .remove(&registration.registration_id());
            if let Some(coordinator) = registration.first_coordinator() {
                hubs.insert(coordinator);
            }
        }
        self.reserved_hub_coordinators
            .retain(|coordinator, _| !hubs.contains(coordinator));

        let mut grants = std::collections::HashSet::new();
        for registration in &block.grant_registrations {
            self.grant_registrations
                .remove(&registration.registration_id());
            grants.insert(registration.registrar);
        }
        self.reserved_grant_registrars
            .retain(|registrar, _| !grants.contains(registrar));

        let mut missions = std::collections::HashSet::new();
        for registration in &block.mission_registrations {
            self.mission_registrations
                .remove(&registration.registration_id());
            missions.insert(registration.sponsor);
        }
        self.reserved_mission_sponsors
            .retain(|sponsor, _| !missions.contains(sponsor));
    }
}

fn select_sorted<T: Clone>(
    map: &std::collections::HashMap<Hash, T>,
    max: usize,
    id_of: fn(&T) -> Hash,
) -> Vec<T> {
    let mut entries: Vec<_> = map.values().cloned().collect();
    entries.sort_by(|a, b| id_of(a).as_bytes().cmp(id_of(b).as_bytes()));
    if entries.len() > max {
        entries.truncate(max);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount, CommunityGrantKind, Hash, TreasuryId};

    fn hub(nonce: u64) -> HubRegistration {
        let mut registration = HubRegistration::unsigned(
            "Agora Hub".into(),
            "Geographic".into(),
            Hash([2; 32]),
            vec![Address([1; 20])],
            Address([4; 20]),
            12,
            3,
            Hash([5; 32]),
            Hash([6; 32]),
            1,
            nonce,
        );
        registration.public_key = vec![1; 33];
        registration.signature = vec![2; 64];
        registration
    }

    #[test]
    fn admits_and_evicts_hub_registrations() {
        let mut pool = Mempool::new(8);
        let first = hub(0);
        let id = pool.admit_hub_registration(first.clone()).unwrap();
        assert!(pool.get_hub_registration(&id).is_some());
        assert!(pool.admit_hub_registration(hub(1)).is_err());
        let mut block = agora_types::Block::utxo(
            agora_types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 1,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        block.hub_registrations.push(first);
        pool.evict_community_registrations_from_block(&block);
        assert!(pool.get_hub_registration(&id).is_none());
        assert!(pool.admit_hub_registration(hub(1)).is_ok());
    }

    #[test]
    fn admits_grant_and_mission_with_nonce_reservation() {
        let mut pool = Mempool::new(8);
        let mut grant = GrantRegistration::unsigned(
            Address([1; 20]),
            7,
            TreasuryId::OvlBuilder,
            Address([2; 20]),
            Amount::from_base_units(10),
            CommunityGrantKind::Micro,
            vec![],
            Hash::ZERO,
            0,
        );
        grant.public_key = vec![1; 33];
        grant.signature = vec![2; 64];
        pool.admit_grant_registration(grant.clone()).unwrap();
        grant.nonce = 1;
        grant.signature = vec![3; 64];
        assert!(pool.admit_grant_registration(grant).is_err());

        let mut mission = MissionRegistration::unsigned(
            Address([1; 20]),
            TreasuryId::DrcCommunity,
            Amount::from_base_units(5),
            Hash([3; 32]),
            0,
        );
        mission.public_key = vec![1; 33];
        mission.signature = vec![2; 64];
        pool.admit_mission_registration(mission).unwrap();
    }
}
