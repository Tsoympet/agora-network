//! Deterministic EIP-1559 base-fee update for `OVL-EVM-v1`.
//!
//! The formula is consensus code. A validator cannot substitute a different
//! denominator, elasticity, or burn ratio. 100% of the base fee is burned.

use agora_types::OvlFeeMarketParams;

use crate::error::EvmError;

/// Next block base fee from the parent header and parent gas used.
///
/// `target = gas_limit / elasticity`. Usage above the target increases the
/// base fee by at least 1 wei. Usage below decreases it without a floor other
/// than zero. A parent base fee of zero stays zero when usage is at or below
/// the target.
pub fn next_base_fee(
    parent_base_fee: u64,
    parent_gas_used: u64,
    params: &OvlFeeMarketParams,
) -> Result<u64, EvmError> {
    params.validate().map_err(EvmError::rejected)?;
    let target = params.gas_target();
    if parent_gas_used == target {
        return Ok(parent_base_fee);
    }
    if parent_gas_used > target {
        let delta = parent_gas_used - target;
        let change = mul_div(
            parent_base_fee,
            delta,
            target,
            params.base_fee_change_denominator,
        )?;
        let change = change.max(1);
        return Ok(parent_base_fee.saturating_add(change));
    }
    let delta = target - parent_gas_used;
    let change = mul_div(
        parent_base_fee,
        delta,
        target,
        params.base_fee_change_denominator,
    )?;
    Ok(parent_base_fee.saturating_sub(change))
}

fn mul_div(parent: u64, delta: u64, target: u64, denominator: u64) -> Result<u64, EvmError> {
    let product = (parent as u128)
        .checked_mul(delta as u128)
        .ok_or_else(|| EvmError::rejected("base fee delta overflow"))?;
    let divisor = (target as u128)
        .checked_mul(denominator as u128)
        .ok_or_else(|| EvmError::rejected("base fee divisor overflow"))?;
    if divisor == 0 {
        return Err(EvmError::rejected("base fee divisor is zero"));
    }
    Ok((product / divisor) as u64)
}

/// Effective price used by the pinned engine.
///
/// Legacy and type-1 transactions pay `gas_price`. Type-2 transactions pay
/// `min(max_fee, base_fee + priority)`.
pub fn effective_gas_price(
    tx_type: u8,
    gas_price_or_max_fee: u128,
    priority: Option<u128>,
    base_fee: u128,
) -> u128 {
    if tx_type == 0 || tx_type == 1 {
        return gas_price_or_max_fee;
    }
    match priority {
        Some(priority) => gas_price_or_max_fee.min(base_fee.saturating_add(priority)),
        None => gas_price_or_max_fee,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_fee_matches_eip1559_integer_rules() {
        let params = OvlFeeMarketParams::dev_default();
        let parent = 1_000u64;
        assert_eq!(
            next_base_fee(parent, params.gas_target(), &params).unwrap(),
            parent
        );
        let up = next_base_fee(parent, params.gas_limit, &params).unwrap();
        assert!(up > parent);
        let down = next_base_fee(parent, 0, &params).unwrap();
        assert!(down < parent);
        assert_eq!(next_base_fee(0, params.gas_limit, &params).unwrap(), 1);
        assert_eq!(next_base_fee(0, 0, &params).unwrap(), 0);
    }
}
