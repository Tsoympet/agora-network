/// Difficulty Adjustment Algorithm parameters for sub-second BlockDAG tips.
///
/// All arithmetic is integer-only so difficulty is deterministic across platforms.
#[derive(Debug, Clone)]
pub struct DaaConfig {
    /// Target spacing between blocks in milliseconds.
    pub target_block_time_ms: u64,
    /// Sliding window length in blue-score units.
    pub window_size: u64,
    /// Clamp ratio when adjusting difficulty (e.g. 2 => at most 2x / floor 1/2x per window).
    pub max_adjustment_factor: u64,
}

impl Default for DaaConfig {
    fn default() -> Self {
        Self {
            target_block_time_ms: 1_000,
            window_size: 90,
            max_adjustment_factor: 2,
        }
    }
}

/// Compact `bits`-style difficulty (higher = harder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Difficulty {
    pub level: u32,
}

impl Default for Difficulty {
    fn default() -> Self {
        Self { level: 8 }
    }
}

/// Compute next difficulty from window timestamps (ms) ordered oldest → newest.
///
/// Uses blue-work windows in production; this scaffold keys off observed timestamps only.
pub fn next_difficulty(
    config: &DaaConfig,
    current: Difficulty,
    window_timestamps_ms: &[u64],
) -> Difficulty {
    if window_timestamps_ms.len() < 2 {
        return current;
    }
    let first = window_timestamps_ms[0];
    let last = *window_timestamps_ms.last().expect("len >= 2");
    if last <= first {
        return current;
    }

    let observed = last - first;
    let intervals = (window_timestamps_ms.len() as u64).saturating_sub(1);
    let expected = config.target_block_time_ms.saturating_mul(intervals).max(1);
    let max_factor = config.max_adjustment_factor.max(1);

    // next ≈ current * expected / observed, clamped to [current/max, current*max].
    let current_u = current.level.max(1) as u64;
    let mut next = current_u.saturating_mul(expected) / observed.max(1);

    let upper = current_u.saturating_mul(max_factor);
    let lower = (current_u / max_factor).max(1);
    if next > upper {
        next = upper;
    } else if next < lower {
        next = lower;
    }

    Difficulty {
        level: next.clamp(1, u32::MAX as u64) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slows_when_blocks_arrive_too_fast() {
        let config = DaaConfig::default();
        let current = Difficulty { level: 10 };
        // 90 intervals in half the expected time => raise difficulty.
        let mut ts = Vec::new();
        for i in 0..91 {
            ts.push(i * (config.target_block_time_ms / 2));
        }
        let next = next_difficulty(&config, current, &ts);
        assert!(next.level > current.level);
    }

    #[test]
    fn eases_when_blocks_arrive_too_slow() {
        let config = DaaConfig::default();
        let current = Difficulty { level: 10 };
        let mut ts = Vec::new();
        for i in 0..91 {
            ts.push(i * (config.target_block_time_ms * 2));
        }
        let next = next_difficulty(&config, current, &ts);
        assert!(next.level < current.level);
    }

    #[test]
    fn integer_math_is_deterministic() {
        let config = DaaConfig::default();
        let current = Difficulty { level: 16 };
        let ts: Vec<u64> = (0..91).map(|i| i * 750).collect();
        let a = next_difficulty(&config, current, &ts);
        let b = next_difficulty(&config, current, &ts);
        assert_eq!(a, b);
    }
}
