//! Ovolos (OVL) native L2 ledger — balances + coinbase mint under supply cap.
//!
//! OVL is **native money on L2** (PoW coinbase + gas). It is not an L1 UTXO asset.
//! Caps align with the genesis registry mark (`TokenMark::ovolos`).

use std::collections::HashMap;

use agora_types::{Address, Amount};
use serde::{Deserialize, Serialize};

use crate::RollupError;

/// Default OVL max supply in base units (21B whole @ 8 decimals).
pub const OVL_MAX_SUPPLY_BASE: u64 = 2_100_000_000_000_000_000;

/// Default gas charged per EVM tx in a batch (base units).
pub const DEFAULT_GAS_PER_TX: u64 = 21_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvlLedger {
    balances: HashMap<Address, u64>,
    minted: u64,
    max_supply: u64,
    pub gas_per_tx: u64,
}

impl Default for OvlLedger {
    fn default() -> Self {
        Self {
            balances: HashMap::new(),
            minted: 0,
            max_supply: OVL_MAX_SUPPLY_BASE,
            gas_per_tx: DEFAULT_GAS_PER_TX,
        }
    }
}

impl OvlLedger {
    pub fn new(max_supply: u64, gas_per_tx: u64) -> Self {
        Self {
            balances: HashMap::new(),
            minted: 0,
            max_supply,
            gas_per_tx,
        }
    }

    pub fn minted(&self) -> u64 {
        self.minted
    }

    pub fn max_supply(&self) -> u64 {
        self.max_supply
    }

    pub fn balance(&self, address: Address) -> Amount {
        Amount::from_base_units(self.balances.get(&address).copied().unwrap_or(0))
    }

    /// Mint OVL under the registry cap (faucet / bridge deposit path).
    pub fn mint(&mut self, to: Address, amount: Amount) -> Result<(), RollupError> {
        let units = amount.as_base_units();
        let next_minted = self
            .minted
            .checked_add(units)
            .ok_or_else(|| RollupError::Execution("OVL mint overflow".into()))?;
        if next_minted > self.max_supply {
            return Err(RollupError::Execution("OVL max supply exceeded".into()));
        }
        let bal = self.balances.entry(to).or_insert(0);
        *bal = bal
            .checked_add(units)
            .ok_or_else(|| RollupError::Execution("OVL balance overflow".into()))?;
        self.minted = next_minted;
        Ok(())
    }

    pub fn transfer(
        &mut self,
        from: Address,
        to: Address,
        amount: Amount,
    ) -> Result<(), RollupError> {
        let units = amount.as_base_units();
        let from_bal = self.balances.get(&from).copied().unwrap_or(0);
        if from_bal < units {
            return Err(RollupError::Execution("insufficient OVL".into()));
        }
        self.balances.insert(from, from_bal - units);
        let to_bal = self.balances.get(&to).copied().unwrap_or(0);
        self.balances.insert(
            to,
            to_bal
                .checked_add(units)
                .ok_or_else(|| RollupError::Execution("OVL balance overflow".into()))?,
        );
        Ok(())
    }

    /// Charge gas for `tx_count` transactions from `payer`.
    pub fn charge_gas(&mut self, payer: Address, tx_count: usize) -> Result<Amount, RollupError> {
        let units = self
            .gas_per_tx
            .checked_mul(tx_count as u64)
            .ok_or_else(|| RollupError::Execution("OVL gas overflow".into()))?;
        let amount = Amount::from_base_units(units);
        let bal = self.balances.get(&payer).copied().unwrap_or(0);
        if bal < units {
            return Err(RollupError::Execution("insufficient OVL for gas".into()));
        }
        self.balances.insert(payer, bal - units);
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_and_gas() {
        let mut ledger = OvlLedger::new(1_000_000, 100);
        let alice = Address([1u8; 20]);
        ledger.mint(alice, Amount::from_base_units(1_000)).unwrap();
        assert_eq!(ledger.balance(alice).as_base_units(), 1_000);
        let gas = ledger.charge_gas(alice, 3).unwrap();
        assert_eq!(gas.as_base_units(), 300);
        assert_eq!(ledger.balance(alice).as_base_units(), 700);
    }

    #[test]
    fn respects_cap() {
        let mut ledger = OvlLedger::new(100, 1);
        let alice = Address([1u8; 20]);
        assert!(ledger.mint(alice, Amount::from_base_units(101)).is_err());
    }
}
