use std::collections::HashMap;
use std::sync::Arc;

use agora_types::{Address, Amount, Hash};

use crate::attestor::AttestorSet;
use crate::district::DistrictConfig;
use crate::drc::{DrcLedger, DRC_MAX_SUPPLY_BASE};
use crate::genesis::{DrachmaGenesis, DEFAULT_DRC_POW_BITS};
use crate::messages::{BridgeDirection, BridgeMessage, MessageStatus};
use crate::pow::{
    messages_root, mine_drc_block, verify_pow, DrcBlock, DrcBlockHeader, DrcEmission,
};
use crate::proof::{verify_inclusion, LightClientProof};
use crate::transport::MessageTransport;
use crate::BridgeError;

/// Default same-district payment fee in base units (XRP drops–style).
pub const DEFAULT_PAYMENT_FEE_BASE: u64 = 10;

#[derive(Debug, Clone)]
struct DistrictTip {
    tip_hash: Hash,
    tip_height: u64,
}

/// Bridge-in-a-Box runtime for District Chain asset moves.
pub struct BridgeBox {
    districts: HashMap<String, DistrictConfig>,
    /// Locked balances on the hub keyed by (district_or_hub, address).
    locks: HashMap<(String, Address), Amount>,
    messages: HashMap<Hash, (BridgeMessage, MessageStatus)>,
    drc: DrcLedger,
    transport: Option<Arc<dyn MessageTransport>>,
    tips: HashMap<String, DistrictTip>,
    pow_bits: u32,
    emission: DrcEmission,
    pow_blocks: HashMap<(String, u64), Hash>,
    /// Bonded attestors (hybrid PoS finality). Empty ⇒ instant finality.
    attestors: AttestorSet,
    /// XRPL-class destination tag registry: (district, tag) → owner address.
    tag_owners: HashMap<(String, u32), Address>,
    /// Payments / mints indexed by (district, tag) for exchange deposit routing.
    tag_payments: HashMap<(String, u32), Vec<Hash>>,
}

impl Default for BridgeBox {
    fn default() -> Self {
        Self {
            districts: HashMap::new(),
            locks: HashMap::new(),
            messages: HashMap::new(),
            drc: DrcLedger::new(DRC_MAX_SUPPLY_BASE),
            transport: None,
            tips: HashMap::new(),
            pow_bits: DEFAULT_DRC_POW_BITS,
            emission: DrcEmission::default(),
            pow_blocks: HashMap::new(),
            attestors: AttestorSet::default(),
            tag_owners: HashMap::new(),
            tag_payments: HashMap::new(),
        }
    }
}

impl BridgeBox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Boot bridge from a frozen Drachma L3 genesis (caps, districts, premine, PoW).
    pub fn from_genesis(genesis: &DrachmaGenesis) -> Result<Self, BridgeError> {
        genesis.validate()?;
        let ledger = genesis.ignite_ledger()?;
        let mut bridge = Self {
            districts: HashMap::new(),
            locks: HashMap::new(),
            messages: HashMap::new(),
            drc: ledger,
            transport: None,
            tips: HashMap::new(),
            pow_bits: genesis.pow_bits,
            emission: genesis.emission(),
            pow_blocks: HashMap::new(),
            attestors: AttestorSet::new(
                &genesis.hub_id,
                crate::attestor::DEFAULT_ATTESTOR_MIN_BOND,
            ),
            tag_owners: HashMap::new(),
            tag_payments: HashMap::new(),
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
        self.tips
            .entry(config.district_id.clone())
            .or_insert(DistrictTip {
                tip_hash: Hash::ZERO,
                tip_height: 0,
            });
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

    pub fn drc_mut(&mut self) -> &mut DrcLedger {
        &mut self.drc
    }

    pub fn attestors(&self) -> &AttestorSet {
        &self.attestors
    }

    pub fn set_attestor_quorum(&mut self, numerator: u32, denominator: u32) {
        self.attestors.quorum_numerator = numerator.max(1);
        self.attestors.quorum_denominator = denominator.max(1);
    }

    /// Bond DRC on the hub as a hybrid attestor (PoS finality).
    pub fn bond_attestor(&mut self, attestor: Address, amount: Amount) -> Result<u64, BridgeError> {
        self.attestors.bond(&mut self.drc, attestor, amount)
    }

    pub fn unbond_attestor(
        &mut self,
        attestor: Address,
        amount: Amount,
    ) -> Result<u64, BridgeError> {
        self.attestors.unbond(&mut self.drc, attestor, amount)
    }

    /// Register an XRPL-class destination tag for deposit routing on `district`.
    ///
    /// Tag `0` is reserved (untagged). Once registered, payments using the tag
    /// must target the registered owner address.
    pub fn register_destination_tag(
        &mut self,
        district: impl Into<String>,
        owner: Address,
        tag: u32,
    ) -> Result<(), BridgeError> {
        let district = district.into();
        if tag == 0 {
            return Err(BridgeError::Constraint(
                "destination tag 0 is reserved".into(),
            ));
        }
        if !self.districts.contains_key(&district) {
            return Err(BridgeError::UnknownDistrict(district));
        }
        let key = (district, tag);
        if let Some(existing) = self.tag_owners.get(&key) {
            if *existing != owner {
                return Err(BridgeError::Constraint(
                    "destination tag already registered to another owner".into(),
                ));
            }
            return Ok(());
        }
        self.tag_owners.insert(key, owner);
        Ok(())
    }

    pub fn destination_tag_owner(&self, district: &str, tag: u32) -> Option<Address> {
        self.tag_owners.get(&(district.to_string(), tag)).copied()
    }

    /// Message ids that used `(district, tag)` (payments / tagged mints).
    pub fn payments_for_tag(&self, district: &str, tag: u32) -> Vec<Hash> {
        self.tag_payments
            .get(&(district.to_string(), tag))
            .cloned()
            .unwrap_or_default()
    }

    fn enforce_tag(&self, district: &str, recipient: Address, tag: u32) -> Result<(), BridgeError> {
        if tag == 0 {
            return Ok(());
        }
        if let Some(owner) = self.tag_owners.get(&(district.to_string(), tag)) {
            if *owner != recipient {
                return Err(BridgeError::Constraint(
                    "destination_tag recipient mismatch".into(),
                ));
            }
        }
        Ok(())
    }

    fn index_tag(&mut self, district: &str, tag: u32, message_id: Hash) {
        if tag == 0 {
            return;
        }
        self.tag_payments
            .entry((district.to_string(), tag))
            .or_default()
            .push(message_id);
    }

    /// Attest a message; on quorum, `Paid` messages become `Finalized`.
    pub fn attest_message(
        &mut self,
        attestor: Address,
        message_id: Hash,
    ) -> Result<bool, BridgeError> {
        if !self.messages.contains_key(&message_id) {
            return Err(BridgeError::NotClaimable("unknown message".into()));
        }
        let reached = self.attestors.attest(attestor, message_id)?;
        if reached {
            if let Some(entry) = self.messages.get_mut(&message_id) {
                if entry.1 == MessageStatus::Paid {
                    entry.1 = MessageStatus::Finalized;
                }
            }
        }
        Ok(reached)
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
        self.lock_and_mint_tagged(
            source_hub,
            dest_district,
            sender,
            recipient,
            amount,
            nonce,
            0,
        )
    }

    /// Lock-and-mint with an XRPL-style destination tag for deposit routing.
    pub fn lock_and_mint_tagged(
        &mut self,
        source_hub: impl Into<String>,
        dest_district: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
    ) -> Result<Hash, BridgeError> {
        let source_hub = source_hub.into();
        let dest_district = dest_district.into();
        if !self.districts.contains_key(&dest_district) {
            return Err(BridgeError::UnknownDistrict(dest_district));
        }
        self.enforce_tag(&dest_district, recipient, destination_tag)?;

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
            destination_tag,
        };
        let id = msg.id();
        if self.messages.contains_key(&id) {
            return Err(BridgeError::DuplicateMessage);
        }
        self.messages
            .insert(id, (msg.clone(), MessageStatus::Locked));
        self.index_tag(&dest_district, destination_tag, id);

        if let Some(transport) = &self.transport {
            transport.publish(&dest_district, msg)?;
        }
        Ok(id)
    }

    /// District acknowledges mint / claim of a LockAndMint message.
    ///
    /// When attestors are bonded, claim requires quorum attestations first.
    pub fn claim_mint(&mut self, message_id: Hash) -> Result<(), BridgeError> {
        if self.attestors.finality_required() && !self.attestors.has_quorum(&message_id) {
            return Err(BridgeError::AwaitingQuorum);
        }
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
        self.burn_and_unlock_tagged(
            source_district,
            dest_hub,
            sender,
            recipient,
            amount,
            nonce,
            0,
        )
    }

    /// Burn-and-unlock with destination tag.
    pub fn burn_and_unlock_tagged(
        &mut self,
        source_district: impl Into<String>,
        dest_hub: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
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
            destination_tag,
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

    /// Same-district DRC payment (XRP Payment–class): account transfer + fee + tag.
    pub fn pay(
        &mut self,
        district: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
    ) -> Result<Hash, BridgeError> {
        let district = district.into();
        if !self.districts.contains_key(&district) {
            return Err(BridgeError::UnknownDistrict(district));
        }
        self.enforce_tag(&district, recipient, destination_tag)?;
        let fee = Amount::from_base_units(DEFAULT_PAYMENT_FEE_BASE);
        self.drc
            .transfer(&district, sender, recipient, amount, fee, None)?;

        let msg = BridgeMessage {
            direction: BridgeDirection::Payment,
            source_district: district.clone(),
            dest_district: district.clone(),
            sender,
            recipient,
            amount,
            nonce,
            destination_tag,
        };
        let id = msg.id();
        if self.messages.contains_key(&id) {
            return Err(BridgeError::DuplicateMessage);
        }
        // Instant finality when no attestors bonded; otherwise await quorum.
        let status = if self.attestors.finality_required() {
            MessageStatus::Paid
        } else {
            MessageStatus::Finalized
        };
        self.messages.insert(id, (msg.clone(), status));
        self.index_tag(&district, destination_tag, id);
        if let Some(transport) = &self.transport {
            transport.publish(&district, msg)?;
        }
        Ok(id)
    }

    /// XRP-style path payment across districts via the hub:
    /// `source → hub (burn/unlock) → dest (lock/mint + claim)`.
    ///
    /// Debits real balances on `source_district` (no faucet minting).
    /// `deliver_min == 0` disables the XRPL deliverMin check; otherwise
    /// `amount` must be ≥ `deliver_min` (full delivery in this 1:1 path model).
    pub fn path_pay(
        &mut self,
        hub: impl Into<String>,
        source_district: impl Into<String>,
        dest_district: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
    ) -> Result<(Hash, Hash), BridgeError> {
        self.path_pay_deliver(
            hub,
            source_district,
            dest_district,
            sender,
            recipient,
            amount,
            nonce,
            destination_tag,
            Amount::ZERO,
        )
    }

    /// Path payment with XRPL-class `deliverMin` floor.
    pub fn path_pay_deliver(
        &mut self,
        hub: impl Into<String>,
        source_district: impl Into<String>,
        dest_district: impl Into<String>,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
        deliver_min: Amount,
    ) -> Result<(Hash, Hash), BridgeError> {
        if deliver_min.as_base_units() > 0 && amount.as_base_units() < deliver_min.as_base_units() {
            return Err(BridgeError::Constraint("deliver_min not met".into()));
        }
        let hub = hub.into();
        let source_district = source_district.into();
        let dest_district = dest_district.into();
        if source_district == dest_district {
            let id = self.pay(
                source_district,
                sender,
                recipient,
                amount,
                nonce,
                destination_tag,
            )?;
            return Ok((id, id));
        }
        let unlock_id = self.burn_and_unlock_tagged(
            source_district,
            hub.clone(),
            sender,
            sender,
            amount,
            nonce,
            0,
        )?;
        let mint_id = self.lock_and_mint_tagged(
            hub,
            dest_district,
            sender,
            recipient,
            amount,
            nonce.saturating_add(1),
            destination_tag,
        )?;
        // With bonded attestors, claim waits for quorum (`attest_message` then `claim_mint`).
        if !self.attestors.finality_required() {
            self.claim_mint(mint_id)?;
        }
        Ok((unlock_id, mint_id))
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

    pub fn pow_bits(&self) -> u32 {
        self.pow_bits
    }

    pub fn emission(&self) -> &DrcEmission {
        &self.emission
    }

    pub fn tip_hash(&self, district_id: &str) -> Option<Hash> {
        self.tips.get(district_id).map(|t| t.tip_hash)
    }

    pub fn tip_height(&self, district_id: &str) -> Option<u64> {
        self.tips.get(district_id).map(|t| t.tip_height)
    }

    /// Admit a mined DRC PoW block for a district/hub: verify PoW, mint coinbase.
    pub fn admit_mined_block(&mut self, block: DrcBlock) -> Result<Hash, BridgeError> {
        verify_pow(&block.header)?;
        if !self.districts.contains_key(&block.header.district_id) {
            return Err(BridgeError::UnknownDistrict(
                block.header.district_id.clone(),
            ));
        }
        if block.header.bits != self.pow_bits {
            return Err(BridgeError::Constraint(format!(
                "DRC PoW bits {} != configured {}",
                block.header.bits, self.pow_bits
            )));
        }
        let tip = self
            .tips
            .get(&block.header.district_id)
            .cloned()
            .unwrap_or(DistrictTip {
                tip_hash: Hash::ZERO,
                tip_height: 0,
            });
        if block.header.height != tip.tip_height {
            return Err(BridgeError::Constraint(format!(
                "DRC block height {} != tip {}",
                block.header.height, tip.tip_height
            )));
        }
        if block.header.prev_block_hash != tip.tip_hash {
            return Err(BridgeError::Constraint(
                "DRC prev_block_hash does not match tip".into(),
            ));
        }
        let root = messages_root(&block.message_ids);
        if root != block.header.messages_root {
            return Err(BridgeError::Constraint("DRC messages_root mismatch".into()));
        }
        let expected_reward = self.emission.reward_at_height(block.header.height);
        if block.header.reward != expected_reward {
            return Err(BridgeError::Constraint(format!(
                "DRC coinbase reward {} != expected {}",
                block.header.reward, expected_reward
            )));
        }
        if expected_reward > 0 {
            self.drc.mint(
                &block.header.district_id,
                block.header.miner,
                Amount::from_base_units(expected_reward),
            )?;
        }
        let id = block.id();
        self.pow_blocks
            .insert((block.header.district_id.clone(), block.header.height), id);
        self.tips.insert(
            block.header.district_id.clone(),
            DistrictTip {
                tip_hash: id,
                tip_height: block.header.height.saturating_add(1),
            },
        );
        Ok(id)
    }

    /// Mine and admit a native DRC PoW block for `district_id`.
    pub fn mine_and_admit(
        &mut self,
        district_id: impl Into<String>,
        message_ids: Vec<Hash>,
        miner: Address,
        timestamp_ms: u64,
        max_nonces: u64,
    ) -> Result<DrcBlock, BridgeError> {
        let district_id = district_id.into();
        if !self.districts.contains_key(&district_id) {
            return Err(BridgeError::UnknownDistrict(district_id));
        }
        let tip = self.tips.get(&district_id).cloned().unwrap_or(DistrictTip {
            tip_hash: Hash::ZERO,
            tip_height: 0,
        });
        let height = tip.tip_height;
        let reward = self.emission.reward_at_height(height);
        let header = DrcBlockHeader {
            district_id,
            height,
            prev_block_hash: tip.tip_hash,
            messages_root: messages_root(&message_ids),
            timestamp_ms,
            bits: self.pow_bits,
            nonce: 0,
            miner,
            reward,
        };
        let block = mine_drc_block(header, message_ids, max_nonces)?;
        self.admit_mined_block(block.clone())?;
        Ok(block)
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

    #[test]
    fn pay_same_district_with_tag_and_fee() {
        let mut bridge = BridgeBox::new();
        bridge.register_district(DistrictConfig::gaming("arena", 9001));
        let alice = Address([1u8; 20]);
        let bob = Address([2u8; 20]);
        bridge
            .drc
            .mint("arena", alice, Amount::from_base_units(1_000))
            .unwrap();
        let id = bridge
            .pay("arena", alice, bob, Amount::from_base_units(100), 1, 42)
            .unwrap();
        assert_eq!(bridge.message_status(&id), Some(MessageStatus::Finalized));
        assert_eq!(bridge.message(&id).unwrap().destination_tag, 42);
        assert_eq!(bridge.drc().balance("arena", bob).as_base_units(), 100);
        assert_eq!(
            bridge.drc().balance("arena", alice).as_base_units(),
            1_000 - 100 - DEFAULT_PAYMENT_FEE_BASE
        );
        assert_eq!(bridge.payments_for_tag("arena", 42), vec![id]);
    }

    #[test]
    fn destination_tag_registry_enforces_owner() {
        let mut bridge = BridgeBox::new();
        bridge.register_district(DistrictConfig::gaming("arena", 9001));
        let exchange = Address([2u8; 20]);
        let alice = Address([1u8; 20]);
        let stranger = Address([3u8; 20]);
        bridge
            .register_destination_tag("arena", exchange, 99)
            .unwrap();
        bridge
            .drc
            .mint("arena", alice, Amount::from_base_units(1_000))
            .unwrap();
        assert!(bridge
            .pay("arena", alice, stranger, Amount::from_base_units(10), 1, 99)
            .is_err());
        let id = bridge
            .pay("arena", alice, exchange, Amount::from_base_units(10), 2, 99)
            .unwrap();
        assert_eq!(bridge.destination_tag_owner("arena", 99), Some(exchange));
        assert_eq!(bridge.payments_for_tag("arena", 99), vec![id]);
    }

    #[test]
    fn path_pay_deliver_min_rejects_shortfall() {
        let mut bridge = BridgeBox::new();
        bridge.register_district(DistrictConfig::general("agora-hub", 1));
        bridge.register_district(DistrictConfig::gaming("arena", 9001));
        bridge.register_district(DistrictConfig::privacy("veil", 9002));
        let alice = Address([1u8; 20]);
        bridge
            .drc
            .mint("arena", alice, Amount::from_whole(10).unwrap())
            .unwrap();
        let err = bridge
            .path_pay_deliver(
                "agora-hub",
                "arena",
                "veil",
                alice,
                alice,
                Amount::from_whole(5).unwrap(),
                1,
                0,
                Amount::from_whole(10).unwrap(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("deliver_min"), "{err}");
    }

    #[test]
    fn path_pay_moves_real_balances_via_hub() {
        let mut bridge = BridgeBox::new();
        bridge.register_district(DistrictConfig::general("agora-hub", 1));
        bridge.register_district(DistrictConfig::gaming("arena", 9001));
        bridge.register_district(DistrictConfig::privacy("veil", 9002));
        let alice = Address([1u8; 20]);
        bridge
            .drc
            .mint("arena", alice, Amount::from_whole(10).unwrap())
            .unwrap();
        let (_u, _m) = bridge
            .path_pay(
                "agora-hub",
                "arena",
                "veil",
                alice,
                alice,
                Amount::from_whole(10).unwrap(),
                7,
                99,
            )
            .unwrap();
        assert_eq!(bridge.drc().balance("arena", alice).as_base_units(), 0);
        assert_eq!(
            bridge.drc().balance("veil", alice).as_base_units(),
            Amount::from_whole(10).unwrap().as_base_units()
        );
    }

    #[test]
    fn attestor_quorum_finalizes_payment_and_gates_claim() {
        let mut bridge = BridgeBox::new();
        bridge.register_district(DistrictConfig::general("agora-hub", 1));
        bridge.register_district(DistrictConfig::gaming("arena", 9001));
        let a = Address([0xA1; 20]);
        let alice = Address([1u8; 20]);
        let bob = Address([2u8; 20]);
        bridge
            .drc
            .mint("agora-hub", a, Amount::from_base_units(10_000_000_000))
            .unwrap();
        bridge
            .bond_attestor(a, Amount::from_base_units(1_000_000_000))
            .unwrap();
        bridge.set_attestor_quorum(1, 1);
        bridge
            .drc
            .mint("arena", alice, Amount::from_base_units(1_000))
            .unwrap();
        let pay_id = bridge
            .pay("arena", alice, bob, Amount::from_base_units(100), 1, 7)
            .unwrap();
        assert_eq!(bridge.message_status(&pay_id), Some(MessageStatus::Paid));
        assert!(bridge.attest_message(a, pay_id).unwrap());
        assert_eq!(
            bridge.message_status(&pay_id),
            Some(MessageStatus::Finalized)
        );

        bridge
            .credit_hub_lock("agora-hub", alice, Amount::from_base_units(50))
            .unwrap();
        let mint_id = bridge
            .lock_and_mint(
                "agora-hub",
                "arena",
                alice,
                bob,
                Amount::from_base_units(50),
                2,
            )
            .unwrap();
        assert!(matches!(
            bridge.claim_mint(mint_id),
            Err(BridgeError::AwaitingQuorum)
        ));
        bridge.attest_message(a, mint_id).unwrap();
        bridge.claim_mint(mint_id).unwrap();
    }

    #[test]
    fn mine_and_admit_mints_native_drc_coinbase() {
        let mut bridge = BridgeBox::new();
        bridge.pow_bits = 0;
        bridge.register_district(DistrictConfig::gaming("arena", 9001));
        let miner = Address([9u8; 20]);
        let before = bridge.drc().balance("arena", miner).as_base_units();
        let block = bridge
            .mine_and_admit("arena", vec![Hash([1u8; 32])], miner, 1, 8)
            .unwrap();
        verify_pow(&block.header).unwrap();
        assert_eq!(bridge.tip_height("arena"), Some(1));
        assert_eq!(bridge.tip_hash("arena"), Some(block.id()));
        let after = bridge.drc().balance("arena", miner).as_base_units();
        assert_eq!(after - before, bridge.emission().reward_at_height(0));
    }
}
