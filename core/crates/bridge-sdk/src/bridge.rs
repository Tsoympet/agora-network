use std::collections::HashMap;
use std::sync::Arc;

use agora_types::{Address, Amount, Hash};

use crate::district::DistrictConfig;
use crate::drc::{DrcLedger, DRC_MAX_SUPPLY_BASE};
use crate::genesis::DrachmaGenesis;
use crate::messages::{BridgeDirection, BridgeMessage, MessageStatus};
use crate::proof::{verify_inclusion, LightClientProof};
use crate::transport::MessageTransport;
use crate::BridgeError;

/// Bridge-in-a-Box runtime for District Chain asset moves.
pub struct BridgeBox {
    districts: HashMap<String, DistrictConfig>,
    /// Locked balances on the hub keyed by (district_or_hub, address).
    locks: HashMap<(String, Address), Amount>,
    messages: HashMap<Hash, (BridgeMessage, MessageStatus)>,
    drc: DrcLedger,
    transport: Option<Arc<dyn MessageTransport>>,
}

impl Default for BridgeBox {
    fn default() -> Self {
        Self {
            districts: HashMap::new(),
            locks: HashMap::new(),
            messages: HashMap::new(),
            drc: DrcLedger::new(DRC_MAX_SUPPLY_BASE),
            transport: None,
        }
    }
}

impl BridgeBox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Boot bridge from a frozen Drachma L3 genesis (caps, districts, premine).
    pub fn from_genesis(genesis: &DrachmaGenesis) -> Result<Self, BridgeError> {
        genesis.validate()?;
        let ledger = genesis.ignite_ledger()?;
        let mut bridge = Self {
            districts: HashMap::new(),
            locks: HashMap::new(),
            messages: HashMap::new(),
            drc: ledger,
            transport: None,
        };
        for cfg in genesis.district_configs()? {
            bridge.register_district(cfg);
        }
        // Hub premine also counts as locked hub liquidity.
        for p in &genesis.premine {
            if p.district_id == genesis.hub_id {
                let addr = Address::from_hex(&p.address_hex)
                    .ok_or_else(|| BridgeError::Constraint("bad premine address".into()))?;
                let key = (genesis.hub_id.clone(), addr);
                let locked = bridge.locks.get(&key).copied().unwrap_or(Amount::ZERO);
                let next = locked
                    .checked_add(Amount::from_base_units(p.amount))
                    .ok_or(BridgeError::InsufficientLock)?;
                bridge.locks.insert(key, next);
            }
        }
        Ok(bridge)
    }

    pub fn with_transport(mut self, transport: Arc<dyn MessageTransport>) -> Self {
        self.transport = Some(transport);
        self
    }

    pub fn set_transport(&mut self, transport: Arc<dyn MessageTransport>) {
        self.transport = Some(transport);
    }

    pub fn register_district(&mut self, config: DistrictConfig) {
        self.districts.insert(config.district_id.clone(), config);
    }

    pub fn district(&self, id: &str) -> Option<&DistrictConfig> {
        self.districts.get(id)
    }

    pub fn districts(&self) -> impl Iterator<Item = &DistrictConfig> {
        self.districts.values()
    }

    pub fn drc(&self) -> &DrcLedger {
        &self.drc
    }

    /// Credit hub locked DRC so `lock_and_mint` can proceed (deposit / faucet).
    pub fn credit_hub_lock(
        &mut self,
        hub: impl Into<String>,
        address: Address,
        amount: Amount,
    ) -> Result<(), BridgeError> {
        let hub = hub.into();
        // Hub credits come from minted DRC held as locks (registry issuance).
        self.drc.mint(&hub, address, amount)?;
        let key = (hub, address);
        let locked = self.locks.get(&key).copied().unwrap_or(Amount::ZERO);
        let next = locked
            .checked_add(amount)
            .ok_or(BridgeError::InsufficientLock)?;
        self.locks.insert(key, next);
        Ok(())
    }

    /// Lock funds on the hub and emit a LockAndMint message toward a district.
    pub fn lock_and_mint(
        &mut self,
        source_hub: impl Into<String>,
        dest_district: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
    ) -> Result<Hash, BridgeError> {
        let source_hub = source_hub.into();
        let dest_district = dest_district.into();
        if !self.districts.contains_key(&dest_district) {
            return Err(BridgeError::UnknownDistrict(dest_district));
        }

        let key = (source_hub.clone(), sender);
        let locked = self.locks.get(&key).copied().unwrap_or(Amount::ZERO);
        let next = locked
            .checked_sub(amount)
            .ok_or(BridgeError::InsufficientLock)?;
        self.locks.insert(key, next);
        // Burn hub-side DRC representation while in flight (re-minted on claim).
        self.drc.burn(&source_hub, sender, amount)?;

        let msg = BridgeMessage {
            direction: BridgeDirection::LockAndMint,
            source_district: source_hub,
            dest_district: dest_district.clone(),
            sender,
            recipient,
            amount,
            nonce,
        };
        let id = msg.id();
        if self.messages.contains_key(&id) {
            return Err(BridgeError::DuplicateMessage);
        }
        self.messages
            .insert(id, (msg.clone(), MessageStatus::Locked));

        if let Some(transport) = &self.transport {
            transport.publish(&dest_district, msg)?;
        }
        Ok(id)
    }

    /// District acknowledges mint / claim of a LockAndMint message.
    pub fn claim_mint(&mut self, message_id: Hash) -> Result<(), BridgeError> {
        let (dest, recipient, amount) = {
            let entry = self
                .messages
                .get_mut(&message_id)
                .ok_or_else(|| BridgeError::NotClaimable("unknown message".into()))?;
            if entry.1 != MessageStatus::Locked || entry.0.direction != BridgeDirection::LockAndMint
            {
                return Err(BridgeError::NotClaimable("not a locked mint".into()));
            }
            let dest = entry.0.dest_district.clone();
            let recipient = entry.0.recipient;
            let amount = entry.0.amount;
            entry.1 = MessageStatus::Claimed;
            (dest, recipient, amount)
        };
        self.drc.mint(&dest, recipient, amount)?;
        Ok(())
    }

    /// Claim only after a light-client inclusion proof verifies against `trusted_root`.
    pub fn claim_mint_with_proof(
        &mut self,
        message_id: Hash,
        proof: &LightClientProof,
        trusted_root: &Hash,
    ) -> Result<(), BridgeError> {
        if proof.message_id != message_id {
            return Err(BridgeError::InvalidProof("message id mismatch".into()));
        }
        if !verify_inclusion(proof, trusted_root) {
            return Err(BridgeError::InvalidProof("inclusion check failed".into()));
        }
        self.claim_mint(message_id)
    }

    /// Burn on district and unlock on hub.
    pub fn burn_and_unlock(
        &mut self,
        source_district: impl Into<String>,
        dest_hub: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
    ) -> Result<Hash, BridgeError> {
        let source_district = source_district.into();
        let dest_hub = dest_hub.into();
        if !self.districts.contains_key(&source_district) {
            return Err(BridgeError::UnknownDistrict(source_district));
        }

        self.drc.burn(&source_district, sender, amount)?;

        let key = (dest_hub.clone(), recipient);
        let locked = self.locks.get(&key).copied().unwrap_or(Amount::ZERO);
        let next = locked
            .checked_add(amount)
            .ok_or(BridgeError::InsufficientLock)?;
        self.locks.insert(key, next);
        self.drc.mint(&dest_hub, recipient, amount)?;

        let msg = BridgeMessage {
            direction: BridgeDirection::BurnAndUnlock,
            source_district: source_district.clone(),
            dest_district: dest_hub,
            sender,
            recipient,
            amount,
            nonce,
        };
        let id = msg.id();
        if self.messages.contains_key(&id) {
            return Err(BridgeError::DuplicateMessage);
        }
        self.messages
            .insert(id, (msg.clone(), MessageStatus::Unlocked));

        if let Some(transport) = &self.transport {
            transport.publish(&source_district, msg)?;
        }
        Ok(id)
    }

    pub fn message_status(&self, id: &Hash) -> Option<MessageStatus> {
        self.messages.get(id).map(|(_, s)| *s)
    }

    pub fn message(&self, id: &Hash) -> Option<&BridgeMessage> {
        self.messages.get(id).map(|(m, _)| m)
    }

    pub fn locked_balance(&self, hub: &str, address: Address) -> Amount {
        self.locks
            .get(&(hub.to_string(), address))
            .copied()
            .unwrap_or(Amount::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::district::DistrictConfig;
    use crate::transport::InMemoryTransport;

    #[test]
    fn lock_claim_burn_roundtrip_with_drc() {
        let transport = Arc::new(InMemoryTransport::new());
        let mut bridge = BridgeBox::new().with_transport(transport.clone());
        bridge.register_district(DistrictConfig::gaming("arena", 9001));

        let alice = Address([1u8; 20]);
        let bob = Address([2u8; 20]);
        let amount = Amount::from_whole(10).unwrap();

        bridge.credit_hub_lock("agora", alice, amount).unwrap();
        assert_eq!(bridge.locked_balance("agora", alice), amount);

        let mint_id = bridge
            .lock_and_mint("agora", "arena", alice, bob, amount, 1)
            .unwrap();
        assert_eq!(bridge.message_status(&mint_id), Some(MessageStatus::Locked));
        assert_eq!(bridge.locked_balance("agora", alice), Amount::ZERO);
        assert!(transport.poll("arena").unwrap().is_some());

        bridge.claim_mint(mint_id).unwrap();
        assert_eq!(
            bridge.message_status(&mint_id),
            Some(MessageStatus::Claimed)
        );
        assert_eq!(bridge.drc().balance("arena", bob), amount);

        let unlock_id = bridge
            .burn_and_unlock("arena", "agora", bob, alice, amount, 2)
            .unwrap();
        assert_eq!(
            bridge.message_status(&unlock_id),
            Some(MessageStatus::Unlocked)
        );
        assert_eq!(bridge.drc().balance("arena", bob), Amount::ZERO);
        assert_eq!(bridge.locked_balance("agora", alice), amount);
    }
}
