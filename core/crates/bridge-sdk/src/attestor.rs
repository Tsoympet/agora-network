//! Hybrid L3 consensus: bonded attestors (PoS-style) + PoW coinbase for miners.
//!
//! - **PoW** still mints DRC via `admit_mined_block` (unchanged).
//! - **Bonded attestors** finalize payments / bridge messages by quorum once the
//!   active set is non-empty. Empty set ⇒ messages finalize immediately.

use std::collections::{HashMap, HashSet};

use agora_types::{Address, Amount, Hash};

use crate::drc::DrcLedger;
use crate::BridgeError;

/// Escrow address holding attestor bonds on the hub district.
pub const ATTESTOR_BOND_ESCROW: Address = Address([0xA0; 20]);

/// Default minimum DRC bond (10 DRC @ 8 decimals).
pub const DEFAULT_ATTESTOR_MIN_BOND: u64 = 1_000_000_000;

/// Default quorum: 2-of-3 of active attestors (numerator/denominator).
pub const DEFAULT_QUORUM_NUMERATOR: u32 = 2;
pub const DEFAULT_QUORUM_DENOMINATOR: u32 = 3;

#[derive(Debug, Clone)]
pub struct AttestorSet {
    pub min_bond: u64,
    pub quorum_numerator: u32,
    pub quorum_denominator: u32,
    /// Hub district id where bonds are escrowed.
    pub hub_id: String,
    bonds: HashMap<Address, u64>,
    /// message_id → attestors who signed
    attestations: HashMap<Hash, HashSet<Address>>,
}

impl Default for AttestorSet {
    fn default() -> Self {
        Self {
            min_bond: DEFAULT_ATTESTOR_MIN_BOND,
            quorum_numerator: DEFAULT_QUORUM_NUMERATOR,
            quorum_denominator: DEFAULT_QUORUM_DENOMINATOR,
            hub_id: "agora-hub".into(),
            bonds: HashMap::new(),
            attestations: HashMap::new(),
        }
    }
}

impl AttestorSet {
    pub fn new(hub_id: impl Into<String>, min_bond: u64) -> Self {
        Self {
            hub_id: hub_id.into(),
            min_bond,
            ..Self::default()
        }
    }

    pub fn bonded(&self, addr: Address) -> u64 {
        self.bonds.get(&addr).copied().unwrap_or(0)
    }

    pub fn is_active(&self, addr: Address) -> bool {
        self.bonded(addr) >= self.min_bond
    }

    pub fn active_attestors(&self) -> Vec<Address> {
        self.bonds
            .iter()
            .filter(|(_, amt)| **amt >= self.min_bond)
            .map(|(a, _)| *a)
            .collect()
    }

    pub fn finality_required(&self) -> bool {
        !self.active_attestors().is_empty()
    }

    pub fn quorum_threshold(&self) -> usize {
        let n = self.active_attestors().len();
        if n == 0 {
            return 0;
        }
        let need =
            (n as u64 * self.quorum_numerator as u64).div_ceil(self.quorum_denominator as u64);
        need.max(1) as usize
    }

    pub fn attestation_count(&self, message_id: &Hash) -> usize {
        self.attestations
            .get(message_id)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    pub fn has_quorum(&self, message_id: &Hash) -> bool {
        if !self.finality_required() {
            return true;
        }
        self.attestation_count(message_id) >= self.quorum_threshold()
    }

    pub fn bond(
        &mut self,
        ledger: &mut DrcLedger,
        attestor: Address,
        amount: Amount,
    ) -> Result<u64, BridgeError> {
        let units = amount.as_base_units();
        if units == 0 {
            return Err(BridgeError::Constraint("zero bond".into()));
        }
        ledger.transfer(
            &self.hub_id,
            attestor,
            ATTESTOR_BOND_ESCROW,
            amount,
            Amount::ZERO,
            None,
        )?;
        let next = self.bonded(attestor).saturating_add(units);
        self.bonds.insert(attestor, next);
        Ok(next)
    }

    pub fn unbond(
        &mut self,
        ledger: &mut DrcLedger,
        attestor: Address,
        amount: Amount,
    ) -> Result<u64, BridgeError> {
        let units = amount.as_base_units();
        let bonded = self.bonded(attestor);
        if units == 0 || units > bonded {
            return Err(BridgeError::Constraint("insufficient bond".into()));
        }
        ledger.transfer(
            &self.hub_id,
            ATTESTOR_BOND_ESCROW,
            attestor,
            amount,
            Amount::ZERO,
            None,
        )?;
        let next = bonded - units;
        if next == 0 {
            self.bonds.remove(&attestor);
        } else {
            self.bonds.insert(attestor, next);
        }
        Ok(next)
    }

    /// Record an attestation. Returns true when quorum is first reached.
    pub fn attest(&mut self, attestor: Address, message_id: Hash) -> Result<bool, BridgeError> {
        if !self.is_active(attestor) {
            return Err(BridgeError::UnauthorizedAttestor);
        }
        let before = self.has_quorum(&message_id);
        self.attestations
            .entry(message_id)
            .or_default()
            .insert(attestor);
        let after = self.has_quorum(&message_id);
        Ok(!before && after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quorum_two_of_three() {
        let mut set = AttestorSet::new("agora-hub", 10);
        set.quorum_numerator = 2;
        set.quorum_denominator = 3;
        let mut ledger = DrcLedger::new(1_000_000);
        let a = Address([1u8; 20]);
        let b = Address([2u8; 20]);
        let c = Address([3u8; 20]);
        for x in [a, b, c] {
            ledger
                .mint("agora-hub", x, Amount::from_base_units(100))
                .unwrap();
            set.bond(&mut ledger, x, Amount::from_base_units(10))
                .unwrap();
        }
        assert_eq!(set.quorum_threshold(), 2);
        let mid = Hash([9u8; 32]);
        assert!(!set.has_quorum(&mid));
        assert!(!set.attest(a, mid).unwrap());
        assert!(set.attest(b, mid).unwrap());
        assert!(set.has_quorum(&mid));
    }
}
