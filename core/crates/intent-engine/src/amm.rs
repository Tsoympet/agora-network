//! Simple constant-product AMM for same-district intents.

use agora_types::Amount;

use crate::IntentError;

/// x * y = k pool denominated in base units.
#[derive(Debug, Clone)]
pub struct ConstantProductPool {
    pub district_id: String,
    pub reserve_give: u64,
    pub reserve_want: u64,
    /// Fee in basis points (e.g. 30 = 0.30%).
    pub fee_bps: u64,
}

impl ConstantProductPool {
    pub fn new(
        district_id: impl Into<String>,
        reserve_give: u64,
        reserve_want: u64,
        fee_bps: u64,
    ) -> Self {
        Self {
            district_id: district_id.into(),
            reserve_give,
            reserve_want,
            fee_bps,
        }
    }

    /// Quote `want` received for `give` input.
    pub fn quote(&self, give: Amount) -> Result<Amount, IntentError> {
        let dx = give.as_base_units();
        if dx == 0 || self.reserve_give == 0 || self.reserve_want == 0 {
            return Err(IntentError::Unsolvable);
        }
        let dx_eff = dx
            .checked_mul(10_000 - self.fee_bps)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(IntentError::Unsolvable)?;
        let numerator = dx_eff
            .checked_mul(self.reserve_want)
            .ok_or(IntentError::Unsolvable)?;
        let denominator = self
            .reserve_give
            .checked_add(dx_eff)
            .ok_or(IntentError::Unsolvable)?;
        let dy = numerator / denominator;
        if dy == 0 {
            return Err(IntentError::Unsolvable);
        }
        Ok(Amount::from_base_units(dy))
    }

    pub fn apply_swap(&mut self, give: Amount) -> Result<Amount, IntentError> {
        let out = self.quote(give)?;
        self.reserve_give = self
            .reserve_give
            .checked_add(give.as_base_units())
            .ok_or(IntentError::Unsolvable)?;
        self.reserve_want = self
            .reserve_want
            .checked_sub(out.as_base_units())
            .ok_or(IntentError::Unsolvable)?;
        Ok(out)
    }
}
