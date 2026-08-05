/// Difficulty Adjustment Algorithm parameters for sub-second BlockDAG tips.
#[derive(Debug, Clone)]
pub struct DaaConfig {
    /// Target spacing between blocks in milliseconds.
    pub target_block_time_ms: u64,
    /// Sliding window length in samples (timestamps), not including the newest alone.
    pub window_size: u64,
    /// Clamp on the per-window **work** change (e.g. 2.0 => at most 2x / 0.5x work).
    ///
    /// `bits` encode leading-zero difficulty, so work scales as `2^bits`. The adjustment
    /// is applied in target/work space and then mapped back to a bit delta of at most
    /// `log2(max_adjustment_factor)` per window (±1 bit at the default 2.0). This keeps
    /// difficulty from jumping by many bits (hundreds/thousands× work) and oscillating.
    pub max_adjustment_factor: f64,
    /// Floor for `Difficulty.level` / `header.bits` (use `0` for unrestricted testnets).
    pub min_level: u32,
}

impl Default for DaaConfig {
    fn default() -> Self {
        Self {
            target_block_time_ms: 1_000,
            window_size: 90,
            max_adjustment_factor: 2.0,
            min_level: 1,
        }
    }
}

/// Compact difficulty: `level` maps 1:1 onto `BlockHeader.bits` (leading-zero requirement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Difficulty {
    pub level: u32,
}

impl Default for Difficulty {
    fn default() -> Self {
        Self { level: 8 }
    }
}

impl Difficulty {
    pub const fn new(level: u32) -> Self {
        Self { level }
    }

    pub const fn as_bits(self) -> u32 {
        self.level
    }
}

/// One sample on the selected-parent spine for work-weighted DAA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaaSample {
    pub timestamp_ms: u64,
    /// Cumulative blue work at this block (monotonic along the spine).
    pub blue_work: u128,
}

/// Approximate work contributed by a block with the given leading-zero `bits`.
pub fn work_from_bits(bits: u32) -> u128 {
    1u128 << bits.min(63)
}

/// Compute next difficulty from window samples ordered oldest → newest.
///
/// Intervals are weighted by blue-work deltas so harder (higher-work) blocks
/// pull the observed spacing more than soft tips.
pub fn next_difficulty_weighted(
    config: &DaaConfig,
    current: Difficulty,
    samples: &[DaaSample],
) -> Difficulty {
    if samples.len() < 2 {
        return current;
    }

    let mut weighted_dt = 0f64;
    let mut total_work = 0f64;
    for window in samples.windows(2) {
        let older = window[0];
        let newer = window[1];
        if newer.timestamp_ms <= older.timestamp_ms {
            continue;
        }
        let work_delta = newer.blue_work.saturating_sub(older.blue_work).max(1) as f64;
        let dt = (newer.timestamp_ms - older.timestamp_ms) as f64;
        weighted_dt += dt * work_delta;
        total_work += work_delta;
    }
    if total_work <= 0.0 || weighted_dt <= 0.0 {
        return current;
    }

    let observed = weighted_dt / total_work;
    // Compare mean observed spacing to target (work-normalized by work weights).
    let expected = config.target_block_time_ms as f64;
    if observed <= 0.0 {
        return current;
    }

    // Adjust in work/target space: desired work multiplier = expected / observed
    // (blocks too fast => observed < expected => factor > 1 => raise work). Clamp the
    // work change, then map to a bit delta via log2 so the encoded leading-zero `bits`
    // move by at most log2(max) per window instead of `level * factor`.
    let max = config.max_adjustment_factor.max(1.0);
    let mut factor = expected / observed;
    if factor > max {
        factor = max;
    } else if factor < 1.0 / max {
        factor = 1.0 / max;
    }

    let delta_bits = factor.log2();
    let next = (current.level as f64 + delta_bits).round() as i64;
    let floored = next.max(config.min_level as i64) as u32;
    Difficulty { level: floored }
}

/// Compute next difficulty from window timestamps (ms) ordered oldest → newest.
///
/// Uniform work per sample — prefer [`next_difficulty_weighted`] with blue-work samples.
pub fn next_difficulty(
    config: &DaaConfig,
    current: Difficulty,
    window_timestamps_ms: &[u64],
) -> Difficulty {
    let samples: Vec<DaaSample> = window_timestamps_ms
        .iter()
        .enumerate()
        .map(|(i, &timestamp_ms)| DaaSample {
            timestamp_ms,
            blue_work: (i as u128) + 1,
        })
        .collect();
    next_difficulty_weighted(config, current, &samples)
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
    fn respects_min_level_zero() {
        // Very slow blocks push difficulty down; it must floor at min_level (0), not go
        // negative. (Additive/log space means level can reach the floor from level 1.)
        let config = DaaConfig {
            min_level: 0,
            ..DaaConfig::default()
        };
        let current = Difficulty { level: 1 };
        let ts: Vec<u64> = (0..91)
            .map(|i| i * (config.target_block_time_ms * 8))
            .collect();
        let next = next_difficulty(&config, current, &ts);
        assert_eq!(next.level, 0);
    }

    #[test]
    fn difficulty_change_is_bounded_to_one_bit_per_window() {
        // Blocks arriving ~100x too fast must NOT jump bits by many (no 256x work spike).
        let config = DaaConfig::default(); // max_adjustment_factor = 2.0 => ±1 bit
        let current = Difficulty { level: 8 };
        let ts: Vec<u64> = (0..91)
            .map(|i| i * (config.target_block_time_ms / 100).max(1))
            .collect();
        let next = next_difficulty(&config, current, &ts);
        assert!(
            next.level <= current.level + 1,
            "difficulty jumped from {} to {} (>1 bit)",
            current.level,
            next.level
        );
        assert!(next.level >= current.level, "should not ease when too fast");
    }

    #[test]
    fn can_rise_from_zero_when_too_fast() {
        // Additive mapping lets difficulty leave 0 (multiplicative `level*factor` could not).
        let config = DaaConfig {
            min_level: 0,
            ..DaaConfig::default()
        };
        let current = Difficulty { level: 0 };
        let ts: Vec<u64> = (0..91)
            .map(|i| i * (config.target_block_time_ms / 4).max(1))
            .collect();
        let next = next_difficulty(&config, current, &ts);
        assert!(next.level >= 1, "difficulty stuck at 0: {}", next.level);
    }

    #[test]
    fn higher_work_tips_weight_spacing_more() {
        let config = DaaConfig {
            target_block_time_ms: 1_000,
            window_size: 2,
            max_adjustment_factor: 4.0,
            min_level: 1,
        };
        let current = Difficulty { level: 10 };
        // Two slow soft blocks then one fast hard block: without work weighting the
        // mean is dominated by the long early gap; with high work on the last tip
        // the short interval pulls difficulty up.
        let soft = vec![
            DaaSample {
                timestamp_ms: 0,
                blue_work: 1,
            },
            DaaSample {
                timestamp_ms: 10_000,
                blue_work: 2,
            },
            DaaSample {
                timestamp_ms: 10_500,
                blue_work: 2 + work_from_bits(20),
            },
        ];
        let next = next_difficulty_weighted(&config, current, &soft);
        assert!(next.level > current.level, "got {}", next.level);
    }
}
