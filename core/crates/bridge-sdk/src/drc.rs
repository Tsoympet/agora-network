//! Drachma (DRC) native L3 ledger — district balances + PoW coinbase under cap.
//!
//! DRC is **native money on L3**. It is not an L1 UTXO asset. Caps align with
//! the genesis registry mark.

use std::collections::HashMap;

use agora_types::{Address, Amount};

use crate::BridgeError;

/// Default DRC max supply in base units (6B whole @ 8 decimals).
pub const DRC_MAX_SUPPLY_BASE: u64 = 600_000_000_000_000_000;

#[derive(Debug, Default, Clone)]
pub struct DrcLedger {
    /// (district_id, address) → balance
    balances: HashMap<(String, Address), u64>,
    minted: u64,
    max_supply: u64,
}

impl DrcLedger {
    pub fn new(max_supply: u64) -> Self {
        Self {
            balances: HashMap::new(),
            minted: 0,
            max_supply,
        }
    }

    pub fn minted(&self) -> u64 {
        self.minted
    }

    pub fn max_supply(&self) -> u64 {
        self.max_supply
    }

    pub fn balance(&self, district: &str, address: Address) -> Amount {
        Amount::from_base_units(
            self.balances
                .get(&(district.to_string(), address))
                .copied()
                .unwrap_or(0),
        )
    }

    pub fn mint(&mut self, district: &str, to: Address, amount: Amount) -> Result<(), BridgeError> {
        let units = amount.as_base_units();
        let next = self
            .minted
            .checked_add(units)
            .ok_or_else(|| BridgeError::Constraint("DRC mint overflow".into()))?;
        if next > self.max_supply {
            return Err(BridgeError::Constraint("DRC max supply exceeded".into()));
        }
        let key = (district.to_string(), to);
        let bal = self.balances.get(&key).copied().unwrap_or(0);
        self.balances.insert(
            key,
            bal.checked_add(units)
                .ok_or_else(|| BridgeError::Constraint("DRC balance overflow".into()))?,
        );
        self.minted = next;
        Ok(())
    }

    pub fn burn(
        &mut self,
        district: &str,
        from: Address,
        amount: Amount,
    ) -> Result<(), BridgeError> {
        let units = amount.as_base_units();
        let key = (district.to_string(), from);
        let bal = self.balances.get(&key).copied().unwrap_or(0);
        if bal < units {
            return Err(BridgeError::InsufficientDistrict);
        }
        self.balances.insert(key, bal - units);
        self.minted = self.minted.saturating_sub(units);
        Ok(())
    }
}
