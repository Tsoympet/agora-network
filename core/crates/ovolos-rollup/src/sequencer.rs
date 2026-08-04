//! Hybrid L2 consensus: bonded sequencers (PoS-style) + PoW coinbase for miners.
//!
//! - **PoW** still mints OVL via `admit_mined_block` (unchanged).
//! - **Bonded sequencers** may submit batches / finalize once the active set is
//!   non-empty. Empty set ⇒ permissionless (dev / bootstrap).

use std::collections::HashMap;

use agora_types::{Address, Amount};

use crate::ovl::OvlLedger;
use crate::RollupError;

/// Escrow address holding sequencer bonds (not a spendable user account).
pub const SEQUENCER_BOND_ESCROW: Address = Address([0xB0; 20]);

/// Default minimum OVL bond to act as sequencer (10 OVL @ 8 decimals).
pub const DEFAULT_SEQUENCER_MIN_BOND: u64 = 1_000_000_000;

#[derive(Debug, Clone)]
pub struct SequencerSet {
    pub min_bond: u64,
    /// Sequencer → bonded base units (escrowed in ledger).
    bonds: HashMap<Address, u64>,
}

impl Default for SequencerSet {
    fn default() -> Self {
        Self {
            min_bond: DEFAULT_SEQUENCER_MIN_BOND,
            bonds: HashMap::new(),
        }
    }
}

impl SequencerSet {
    pub fn new(min_bond: u64) -> Self {
        Self {
            min_bond,
            bonds: HashMap::new(),
        }
    }

    pub fn bonded(&self, addr: Address) -> u64 {
        self.bonds.get(&addr).copied().unwrap_or(0)
    }

    pub fn is_active(&self, addr: Address) -> bool {
        self.bonded(addr) >= self.min_bond
    }

    pub fn active_sequencers(&self) -> Vec<Address> {
        self.bonds
            .iter()
            .filter(|(_, amt)| **amt >= self.min_bond)
            .map(|(a, _)| *a)
            .collect()
    }

    /// When any sequencer is active, batch submit/finalize require a bonded one.
    pub fn authorization_required(&self) -> bool {
        !self.active_sequencers().is_empty()
    }

    pub fn authorize(&self, sequencer: Address) -> Result<(), RollupError> {
        if !self.authorization_required() {
            return Ok(());
        }
        if self.is_active(sequencer) {
            Ok(())
        } else {
            Err(RollupError::UnauthorizedSequencer)
        }
    }

    /// Lock `amount` OVL from `sequencer` into the bond escrow.
    pub fn bond(
        &mut self,
        ledger: &mut OvlLedger,
        sequencer: Address,
        amount: Amount,
    ) -> Result<u64, RollupError> {
        let units = amount.as_base_units();
        if units == 0 {
            return Err(RollupError::Execution("zero bond".into()));
        }
        ledger.transfer(sequencer, SEQUENCER_BOND_ESCROW, amount)?;
        let next = self.bonded(sequencer).saturating_add(units);
        self.bonds.insert(sequencer, next);
        Ok(next)
    }

    /// Release up to `amount` from bond back to `sequencer` (must remain inactive
    /// or keep ≥ min_bond if still active — partial unbond allowed).
    pub fn unbond(
        &mut self,
        ledger: &mut OvlLedger,
        sequencer: Address,
        amount: Amount,
    ) -> Result<u64, RollupError> {
        let units = amount.as_base_units();
        let bonded = self.bonded(sequencer);
        if units == 0 || units > bonded {
            return Err(RollupError::Execution("insufficient bond".into()));
        }
        ledger.transfer(SEQUENCER_BOND_ESCROW, sequencer, amount)?;
        let next = bonded - units;
        if next == 0 {
            self.bonds.remove(&sequencer);
        } else {
            self.bonds.insert(sequencer, next);
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bond_gates_authorization() {
        let mut set = SequencerSet::new(100);
        let mut ledger = OvlLedger::new(1_000_000, 1);
        let seq = Address([1u8; 20]);
        ledger.mint(seq, Amount::from_base_units(500)).unwrap();
        assert!(!set.authorization_required());
        set.bond(&mut ledger, seq, Amount::from_base_units(100))
            .unwrap();
        assert!(set.authorization_required());
        assert!(set.authorize(seq).is_ok());
        assert!(set.authorize(Address([2u8; 20])).is_err());
    }
}
