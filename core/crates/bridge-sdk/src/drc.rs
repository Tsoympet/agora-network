//! Drachma (DRC) native L3 ledger — district balances + PoW coinbase under cap.
//!
//! DRC is **native money on L3**. It is not an L1 UTXO asset. Caps align with
//! the genesis registry mark.

use std::collections::HashMap;

use agora_types::{Address, Amount};
use serde::{Deserialize, Serialize};

use crate::BridgeError;

/// Default DRC max supply in base units (6B whole @ 8 decimals).
pub const DRC_MAX_SUPPLY_BASE: u64 = 600_000_000_000_000_000;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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

    pub fn balances_snapshot(&self) -> Vec<(String, Address, u64)> {
        self.balances
            .iter()
            .map(|((d, a), v)| (d.clone(), *a, *v))
            .collect()
    }

    pub fn restore_balances(&mut self, balances: Vec<(String, Address, u64)>, minted: u64) {
        self.balances = balances.into_iter().map(|(d, a, v)| ((d, a), v)).collect();
        self.minted = minted;
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

    /// Same-district account payment (XRP Payment–class primitive).
    ///
    /// Moves `amount` from `from` to `to` and optionally charges `fee` to `fee_sink`
    /// (burned from `from` without increasing `to`).
    pub fn transfer(
        &mut self,
        district: &str,
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
        fee_sink: Option<Address>,
    ) -> Result<(), BridgeError> {
        let pay = amount.as_base_units();
        let fee_u = fee.as_base_units();
        let total = pay
            .checked_add(fee_u)
            .ok_or_else(|| BridgeError::Constraint("DRC payment overflow".into()))?;
        let from_key = (district.to_string(), from);
        let from_bal = self.balances.get(&from_key).copied().unwrap_or(0);
        if from_bal < total {
            return Err(BridgeError::InsufficientDistrict);
        }
        self.balances.insert(from_key, from_bal - total);

        let to_key = (district.to_string(), to);
        let to_bal = self.balances.get(&to_key).copied().unwrap_or(0);
        self.balances.insert(
            to_key,
            to_bal
                .checked_add(pay)
                .ok_or_else(|| BridgeError::Constraint("DRC balance overflow".into()))?,
        );

        if fee_u > 0 {
            if let Some(sink) = fee_sink {
                let sink_key = (district.to_string(), sink);
                let sink_bal = self.balances.get(&sink_key).copied().unwrap_or(0);
                self.balances.insert(
                    sink_key,
                    sink_bal
                        .checked_add(fee_u)
                        .ok_or_else(|| BridgeError::Constraint("DRC fee sink overflow".into()))?,
                );
            } else {
                // Fee burned (removed from circulating minted supply).
                self.minted = self.minted.saturating_sub(fee_u);
            }
        }
        Ok(())
    }
}
