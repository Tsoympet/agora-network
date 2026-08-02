use std::collections::HashMap;

use agora_types::{Address, Amount, Hash};

use crate::district::DistrictConfig;
use crate::messages::{BridgeDirection, BridgeMessage, MessageStatus};
use crate::BridgeError;

/// In-memory Bridge-in-a-Box runtime for District Chain asset moves.
#[derive(Debug, Default)]
pub struct BridgeBox {
    districts: HashMap<String, DistrictConfig>,
    /// Locked balances on the hub keyed by (district, address).
    locks: HashMap<(String, Address), Amount>,
    messages: HashMap<Hash, (BridgeMessage, MessageStatus)>,
}

impl BridgeBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_district(&mut self, config: DistrictConfig) {
        self.districts.insert(config.district_id.clone(), config);
    }

    pub fn district(&self, id: &str) -> Option<&DistrictConfig> {
        self.districts.get(id)
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
            .checked_add(amount)
            .ok_or(BridgeError::InsufficientLock)?;
        self.locks.insert(key, next);

        let msg = BridgeMessage {
            direction: BridgeDirection::LockAndMint,
            source_district: source_hub,
            dest_district,
            sender,
            recipient,
            amount,
            nonce,
        };
        let id = msg.id();
        if self.messages.contains_key(&id) {
            return Err(BridgeError::DuplicateMessage);
        }
        self.messages.insert(id, (msg, MessageStatus::Locked));
        Ok(id)
    }

    /// District acknowledges mint / claim of a LockAndMint message.
    pub fn claim_mint(&mut self, message_id: Hash) -> Result<(), BridgeError> {
        let entry = self
            .messages
            .get_mut(&message_id)
            .ok_or_else(|| BridgeError::NotClaimable("unknown message".into()))?;
        if entry.1 != MessageStatus::Locked || entry.0.direction != BridgeDirection::LockAndMint {
            return Err(BridgeError::NotClaimable("not a locked mint".into()));
        }
        entry.1 = MessageStatus::Claimed;
        Ok(())
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

        let key = (dest_hub.clone(), recipient);
        let locked = self.locks.get(&key).copied().unwrap_or(Amount::ZERO);
        let next = locked
            .checked_sub(amount)
            .ok_or(BridgeError::InsufficientLock)?;
        self.locks.insert(key, next);

        let msg = BridgeMessage {
            direction: BridgeDirection::BurnAndUnlock,
            source_district,
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
        self.messages.insert(id, (msg, MessageStatus::Unlocked));
        Ok(id)
    }

    pub fn message_status(&self, id: &Hash) -> Option<MessageStatus> {
        self.messages.get(id).map(|(_, s)| *s)
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

    #[test]
    fn lock_claim_burn_roundtrip() {
        let mut bridge = BridgeBox::new();
        bridge.register_district(DistrictConfig::gaming("arena", 9001));

        let alice = Address([1u8; 20]);
        let bob = Address([2u8; 20]);
        let amount = Amount::from_whole(10).unwrap();

        let mint_id = bridge
            .lock_and_mint("agora", "arena", alice, bob, amount, 1)
            .unwrap();
        assert_eq!(bridge.message_status(&mint_id), Some(MessageStatus::Locked));
        assert_eq!(bridge.locked_balance("agora", alice), amount);

        bridge.claim_mint(mint_id).unwrap();
        assert_eq!(bridge.message_status(&mint_id), Some(MessageStatus::Claimed));

        let unlock_id = bridge
            .burn_and_unlock("arena", "agora", bob, alice, amount, 2)
            .unwrap();
        assert_eq!(
            bridge.message_status(&unlock_id),
            Some(MessageStatus::Unlocked)
        );
        assert_eq!(bridge.locked_balance("agora", alice), Amount::ZERO);
    }
}
