use std::collections::HashMap;
use std::time::{Duration, Instant};

use agora_rpc::{InMemoryBackend, RpcBackend};
use agora_types::{Address, Amount};

use crate::error::{FaucetError, Result};

/// Faucet drip policy.
#[derive(Debug, Clone)]
pub struct FaucetConfig {
    /// Base units credited per successful drip.
    pub drip_amount: Amount,
    /// Minimum time between drips for the same address.
    pub cooldown: Duration,
    /// Optional hard cap on total faucet spend (None = unlimited).
    pub max_total: Option<Amount>,
}

impl Default for FaucetConfig {
    fn default() -> Self {
        Self {
            drip_amount: Amount::from_whole(10).expect("10 AGORA"),
            cooldown: Duration::from_secs(60),
            max_total: None,
        }
    }
}

/// Rate-limited faucet backed by an in-memory RPC ledger (testnet scaffold).
#[derive(Debug)]
pub struct FaucetService {
    config: FaucetConfig,
    backend: InMemoryBackend,
    last_drip: HashMap<Address, Instant>,
    total_dispensed: Amount,
}

impl FaucetService {
    pub fn new(config: FaucetConfig) -> Self {
        Self {
            config,
            backend: InMemoryBackend::new(),
            last_drip: HashMap::new(),
            total_dispensed: Amount::ZERO,
        }
    }

    pub fn with_backend(config: FaucetConfig, backend: InMemoryBackend) -> Self {
        Self {
            config,
            backend,
            last_drip: HashMap::new(),
            total_dispensed: Amount::ZERO,
        }
    }

    pub fn backend(&self) -> &InMemoryBackend {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut InMemoryBackend {
        &mut self.backend
    }

    pub fn balance(&self, address: &Address) -> Amount {
        self.backend.get_balance(address)
    }

    pub fn drip(&mut self, address: Address) -> Result<Amount> {
        if let Some(last) = self.last_drip.get(&address) {
            let elapsed = last.elapsed();
            if elapsed < self.config.cooldown {
                let remain = (self.config.cooldown - elapsed).as_secs().max(1);
                return Err(FaucetError::RateLimited(remain));
            }
        }

        if let Some(max) = self.config.max_total {
            let next = self
                .total_dispensed
                .checked_add(self.config.drip_amount)
                .ok_or(FaucetError::Exhausted)?;
            if next > max {
                return Err(FaucetError::Exhausted);
            }
        }

        let new_balance = self
            .backend
            .fund_address(address, self.config.drip_amount)
            .map_err(|e| FaucetError::Backend(e.to_string()))?;

        self.last_drip.insert(address, Instant::now());
        self.total_dispensed = self
            .total_dispensed
            .checked_add(self.config.drip_amount)
            .unwrap_or(self.total_dispensed);
        Ok(new_balance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drip_and_rate_limit() {
        let mut faucet = FaucetService::new(FaucetConfig {
            drip_amount: Amount::from_base_units(1_000),
            cooldown: Duration::from_secs(3600),
            max_total: Some(Amount::from_base_units(2_000)),
        });
        let addr = Address::from_hex("0102030405060708090a0b0c0d0e0f1011121314").unwrap();
        assert_eq!(faucet.drip(addr).unwrap().as_base_units(), 1_000);
        assert!(matches!(
            faucet.drip(addr),
            Err(FaucetError::RateLimited(_))
        ));
    }
}
