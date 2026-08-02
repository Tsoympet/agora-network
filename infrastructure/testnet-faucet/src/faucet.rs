use std::collections::HashMap;
use std::time::{Duration, Instant};

use agora_rpc::{InMemoryBackend, RpcBackend};
use agora_types::{Address, Amount};

use crate::error::{FaucetError, Result};
use crate::node_rpc;

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

/// Where drip credits are applied.
#[derive(Debug)]
pub enum FundingTarget {
    /// Unit-test / offline ledger (not spendable on a live node).
    Memory(InMemoryBackend),
    /// Live `agora-node` via `agora_fundAddress` (mints spendable `cf_utxo`).
    Node { rpc_url: String },
}

/// Rate-limited faucet that credits either memory or a live node.
#[derive(Debug)]
pub struct FaucetService {
    config: FaucetConfig,
    target: FundingTarget,
    last_drip: HashMap<Address, Instant>,
    total_dispensed: Amount,
}

impl FaucetService {
    pub fn memory(config: FaucetConfig) -> Self {
        Self {
            config,
            target: FundingTarget::Memory(InMemoryBackend::new()),
            last_drip: HashMap::new(),
            total_dispensed: Amount::ZERO,
        }
    }

    pub fn node(config: FaucetConfig, rpc_url: impl Into<String>) -> Self {
        Self {
            config,
            target: FundingTarget::Node {
                rpc_url: rpc_url.into(),
            },
            last_drip: HashMap::new(),
            total_dispensed: Amount::ZERO,
        }
    }

    /// Backward-compatible constructor (in-memory ledger).
    pub fn new(config: FaucetConfig) -> Self {
        Self::memory(config)
    }

    pub fn with_backend(config: FaucetConfig, backend: InMemoryBackend) -> Self {
        Self {
            config,
            target: FundingTarget::Memory(backend),
            last_drip: HashMap::new(),
            total_dispensed: Amount::ZERO,
        }
    }

    pub async fn balance(&self, address: &Address) -> Result<Amount> {
        match &self.target {
            FundingTarget::Memory(backend) => Ok(backend.get_balance(address)),
            FundingTarget::Node { rpc_url } => node_rpc::get_balance(rpc_url, address)
                .await
                .map_err(FaucetError::Backend),
        }
    }

    pub async fn drip(&mut self, address: Address) -> Result<Amount> {
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

        let new_balance = match &mut self.target {
            FundingTarget::Memory(backend) => backend
                .fund_address(address, self.config.drip_amount)
                .map_err(|e| FaucetError::Backend(e.to_string()))?,
            FundingTarget::Node { rpc_url } => {
                node_rpc::fund_address(rpc_url, address, self.config.drip_amount)
                    .await
                    .map_err(FaucetError::Backend)?
            }
        };

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

    #[tokio::test]
    async fn drip_and_rate_limit() {
        let mut faucet = FaucetService::memory(FaucetConfig {
            drip_amount: Amount::from_base_units(1_000),
            cooldown: Duration::from_secs(3600),
            max_total: Some(Amount::from_base_units(2_000)),
        });
        let addr = Address::from_hex("0102030405060708090a0b0c0d0e0f1011121314").unwrap();
        assert_eq!(faucet.drip(addr).await.unwrap().as_base_units(), 1_000);
        assert!(matches!(
            faucet.drip(addr).await,
            Err(FaucetError::RateLimited(_))
        ));
    }
}
