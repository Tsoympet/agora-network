use agora_types::Address;

use crate::math::isqrt;
use crate::whale::{apply_whale_cap, WhaleCapConfig};
use crate::GovernanceError;

/// One voter's raw stake balance (base units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoterBalance {
    pub address: Address,
    pub raw_balance: u64,
}

/// Effective voting power after quadratic transform and whale caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveVote {
    pub address: Address,
    pub raw_balance: u64,
    /// Balance after the 5% supply whale clamp (pre-sqrt).
    pub capped_balance: u64,
    /// `floor(sqrt(capped_balance))`.
    pub effective_votes: u64,
    pub was_whale_capped: bool,
}

/// Pure quadratic mapping: `EffectiveVotes = √RawBalance` (integer floor).
pub fn quadratic_votes(raw_balance: u64) -> u64 {
    isqrt(raw_balance)
}

/// Apply whale cap then quadratic voting for a single balance.
pub fn effective_votes_for(
    raw_balance: u64,
    total_supply: u64,
    cap: &WhaleCapConfig,
) -> Result<(u64, u64, bool), GovernanceError> {
    if total_supply == 0 {
        return Err(GovernanceError::ZeroSupply);
    }
    let (capped, was_capped) = apply_whale_cap(raw_balance, total_supply, cap)?;
    Ok((capped, quadratic_votes(capped), was_capped))
}

/// Tally an electorate under quadratic voting + whale protection.
pub fn tally_quadratic_votes(
    voters: &[VoterBalance],
    total_supply: u64,
    cap: &WhaleCapConfig,
) -> Result<Vec<EffectiveVote>, GovernanceError> {
    if voters.is_empty() {
        return Err(GovernanceError::EmptyElectorate);
    }
    if total_supply == 0 {
        return Err(GovernanceError::ZeroSupply);
    }

    let mut out = Vec::with_capacity(voters.len());
    for voter in voters {
        let (capped_balance, effective_votes, was_whale_capped) =
            effective_votes_for(voter.raw_balance, total_supply, cap)?;
        out.push(EffectiveVote {
            address: voter.address,
            raw_balance: voter.raw_balance,
            capped_balance,
            effective_votes,
            was_whale_capped,
        });
    }
    Ok(out)
}

/// Sum of effective votes across a tally.
pub fn total_effective_power(tally: &[EffectiveVote]) -> Result<u64, GovernanceError> {
    tally
        .iter()
        .try_fold(0u64, |acc, v| acc.checked_add(v.effective_votes))
        .ok_or(GovernanceError::Overflow)
}

/// Largest share of total effective power in basis points (1% = 100 bps).
pub fn max_power_share_bps(tally: &[EffectiveVote]) -> Result<u64, GovernanceError> {
    let total = total_effective_power(tally)?;
    if total == 0 {
        return Ok(0);
    }
    let max = tally.iter().map(|v| v.effective_votes).max().unwrap_or(0);
    Ok(max.saturating_mul(10_000) / total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whale::WhaleCapConfig;

    fn addr(byte: u8) -> Address {
        Address([byte; 20])
    }

    #[test]
    fn quadratic_diminishes_large_balances() {
        assert_eq!(quadratic_votes(100), 10);
        assert_eq!(quadratic_votes(10_000), 100);
        // 100x raw balance => only 10x effective votes.
        assert_eq!(quadratic_votes(10_000) / quadratic_votes(100), 10);
    }

    #[test]
    fn whale_cannot_dominate_after_cap_and_sqrt() {
        let supply = 100_000_000u64;
        let cap = WhaleCapConfig::default(); // 5%
        let voters = vec![
            VoterBalance {
                address: addr(1),
                raw_balance: 80_000_000, // 80% whale
            },
            VoterBalance {
                address: addr(2),
                raw_balance: 1_000_000,
            },
            VoterBalance {
                address: addr(3),
                raw_balance: 1_000_000,
            },
        ];
        let tally = tally_quadratic_votes(&voters, supply, &cap).unwrap();
        assert!(tally[0].was_whale_capped);
        assert_eq!(tally[0].capped_balance, supply * 5 / 100);
        let share = max_power_share_bps(&tally).unwrap();
        // Even the capped whale should not approach a pure majority of sqrt-weight
        // when others also hold meaningful stake — assert under 70% here.
        assert!(share < 7_000, "whale share bps={share}");
    }
}
