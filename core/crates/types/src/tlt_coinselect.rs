//! Deterministic TLT wallet coin selection.
//!
//! This is wallet policy. It does not change which transactions are valid.
//! The search prefers zero change, then smaller change, then fewer inputs.
//! At most [`TLT_COINSELECT_EXHAUSTIVE_CAP`] of the largest coins are searched
//! exhaustively. If those cannot pay, selection falls back to largest-first.

use crate::OutPoint;

/// Largest coins considered by the exhaustive search.
pub const TLT_COINSELECT_EXHAUSTIVE_CAP: usize = 12;

/// One spendable TLT output known to the wallet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TltSpendCoin {
    pub outpoint: OutPoint,
    pub value: u64,
}

/// Selected inputs and the change that remains after `amount + fee`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TltCoinSelection {
    pub coins: Vec<TltSpendCoin>,
    pub total_in: u64,
    pub change: u64,
}

/// Why selection failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TltCoinSelectError {
    #[error("amount must be greater than zero")]
    ZeroAmount,
    #[error("amount plus fee overflows")]
    Overflow,
    #[error("not enough value to cover amount and fee")]
    Insufficient,
}

/// Select coins for `amount` plus an explicit `fee` (base units).
pub fn select_tlt_coins(
    coins: &[TltSpendCoin],
    amount: u64,
    fee: u64,
) -> Result<TltCoinSelection, TltCoinSelectError> {
    if amount == 0 {
        return Err(TltCoinSelectError::ZeroAmount);
    }
    let target = amount
        .checked_add(fee)
        .ok_or(TltCoinSelectError::Overflow)?;
    let mut ordered = coins.to_vec();
    ordered.sort_by(|a, b| {
        b.value
            .cmp(&a.value)
            .then_with(|| a.outpoint.tx_id.as_bytes().cmp(b.outpoint.tx_id.as_bytes()))
            .then_with(|| a.outpoint.index.cmp(&b.outpoint.index))
    });
    let window = ordered.len().min(TLT_COINSELECT_EXHAUSTIVE_CAP);
    if let Some(selection) = exhaustive(&ordered[..window], target) {
        return Ok(selection);
    }
    largest_first(&ordered, target).ok_or(TltCoinSelectError::Insufficient)
}

fn exhaustive(coins: &[TltSpendCoin], target: u64) -> Option<TltCoinSelection> {
    let n = coins.len();
    if n == 0 || n > TLT_COINSELECT_EXHAUSTIVE_CAP {
        return None;
    }
    let mut best: Option<(u64, usize, Vec<usize>)> = None;
    let width = u32::try_from(n).expect("coin window fits u32");
    let limit = 1u32 << width;
    for mask in 1..limit {
        let mut sum = 0u64;
        let mut idxs = Vec::new();
        let mut overflow = false;
        for (i, coin) in coins.iter().enumerate() {
            if mask & (1u32 << u32::try_from(i).expect("coin index")) == 0 {
                continue;
            }
            match sum.checked_add(coin.value) {
                Some(next) => sum = next,
                None => {
                    overflow = true;
                    break;
                }
            }
            idxs.push(i);
        }
        if overflow || sum < target {
            continue;
        }
        let change = sum - target;
        let better = match &best {
            None => true,
            Some((best_change, best_len, best_idxs)) => {
                change < *best_change
                    || (change == *best_change && idxs.len() < *best_len)
                    || (change == *best_change && idxs.len() == *best_len && idxs < *best_idxs)
            }
        };
        if better {
            best = Some((change, idxs.len(), idxs));
        }
    }
    let (_, _, idxs) = best?;
    finish(coins, &idxs, target)
}

fn largest_first(coins: &[TltSpendCoin], target: u64) -> Option<TltCoinSelection> {
    let mut sum = 0u64;
    let mut idxs = Vec::new();
    for (i, coin) in coins.iter().enumerate() {
        sum = sum.checked_add(coin.value)?;
        idxs.push(i);
        if sum >= target {
            return finish(coins, &idxs, target);
        }
    }
    None
}

fn finish(coins: &[TltSpendCoin], idxs: &[usize], target: u64) -> Option<TltCoinSelection> {
    let selected: Vec<TltSpendCoin> = idxs.iter().map(|i| coins[*i]).collect();
    let total_in = selected
        .iter()
        .try_fold(0u64, |acc, coin| acc.checked_add(coin.value))?;
    Some(TltCoinSelection {
        coins: selected,
        total_in,
        change: total_in - target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash;

    fn coin(byte: u8, index: u32, value: u64) -> TltSpendCoin {
        TltSpendCoin {
            outpoint: OutPoint {
                tx_id: Hash([byte; 32]),
                index,
            },
            value,
        }
    }

    #[test]
    fn prefers_exact_change_over_largest_first() {
        let coins = [
            coin(0xAA, 0, 5),
            coin(0xBB, 0, 4),
            coin(0xCC, 1, 3),
            coin(0xDD, 0, 1),
        ];
        let selected = select_tlt_coins(&coins, 5, 1).unwrap();
        assert_eq!(selected.change, 0);
        assert_eq!(selected.total_in, 6);
        assert_eq!(
            selected.coins.iter().map(|c| c.value).collect::<Vec<_>>(),
            vec![5, 1]
        );
    }

    #[test]
    fn rejects_empty_amount_and_insufficient_funds() {
        assert_eq!(
            select_tlt_coins(&[coin(1, 0, 5)], 0, 1),
            Err(TltCoinSelectError::ZeroAmount)
        );
        assert_eq!(
            select_tlt_coins(&[coin(1, 0, 5)], 5, 1),
            Err(TltCoinSelectError::Insufficient)
        );
    }

    #[test]
    fn falls_back_to_largest_first_when_the_window_cannot_pay() {
        let mut coins = Vec::new();
        for i in 0..TLT_COINSELECT_EXHAUSTIVE_CAP {
            coins.push(coin(u8::try_from(i + 1).unwrap(), 0, 1));
        }
        coins.push(coin(0xFF, 0, 100));
        let selected = select_tlt_coins(&coins, 100, 0).unwrap();
        assert_eq!(selected.coins.len(), 1);
        assert_eq!(selected.coins[0].value, 100);
        assert_eq!(selected.change, 0);
    }
}
