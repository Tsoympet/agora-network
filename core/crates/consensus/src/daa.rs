//! Difficulty Adjustment Algorithm for BlockDAG selected-parent spines.
//!
//! Expected bits for a candidate are derived from its **canonical selected parent**
//! context, never from whichever tip the local node retargeted most recently.

/// Difficulty Adjustment Algorithm parameters for sub-second BlockDAG tips.
#[derive(Debug, Clone)]
pub struct DaaConfig {
    /// Target spacing between blocks in milliseconds.
    pub target_block_time_ms: u64,
    /// Sliding window length in samples (timestamps), not including the newest alone.
    pub window_size: u64,
    /// Clamp on the per-window **work** change (e.g. 2.0 => at most 2x / 0.5x work).
    ///
    /// Mapped to a bit delta of at most `log2(max_adjustment_factor)` per window
    /// (±1 bit at the default 2.0).
    pub max_adjustment_factor: f64,
    /// Floor for `Difficulty.level` / `header.bits` (use `0` for unrestricted testnets).
    pub min_level: u32,
    /// Ceiling for `Difficulty.level` / `header.bits` (hard cap against runaway retargets).
    pub max_level: u32,
}

impl Default for DaaConfig {
    fn default() -> Self {
        Self {
            target_block_time_ms: 1_000,
            window_size: 90,
            max_adjustment_factor: 2.0,
            min_level: 1,
            max_level: 128,
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

    pub fn clamp(self, config: &DaaConfig) -> Self {
        Self {
            level: self.level.clamp(config.min_level, config.max_level),
        }
    }
}

/// One sample on the selected-parent spine for hashrate DAA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaaSample {
    pub timestamp_ms: u64,
    /// Cumulative accepted blue work at this block (monotonic along the spine).
    pub blue_work: u128,
    /// This block's own PoW contribution (`work_from_bits(header.bits)`).
    pub block_work: u128,
}

/// Approximate work contributed by a block with the given leading-zero `bits`.
///
/// Uses saturating shifts so bits above 127 still contribute maximal `u128` work
/// instead of wrapping or stalling at bit 63.
pub fn work_from_bits(bits: u32) -> u128 {
    if bits == 0 {
        return 1;
    }
    if bits >= 128 {
        return u128::MAX;
    }
    1u128 << bits
}

/// Median of up to the last 11 timestamps (Bitcoin-style MTP), or `0` if empty.
pub fn median_time_past(timestamps_newest_first: &[u64]) -> u64 {
    let n = timestamps_newest_first.len().min(11);
    if n == 0 {
        return 0;
    }
    let mut window: Vec<u64> = timestamps_newest_first[..n].to_vec();
    window.sort_unstable();
    window[n / 2]
}

/// Next difficulty from selected-parent spine samples (oldest → newest).
///
/// Uses `observed_hashrate = total_window_work / robust_elapsed` and
/// `next_target_work ≈ observed_hashrate × target_block_time`, then maps the
/// work ratio onto a bounded bit delta. Equal / non-increasing timestamps are
/// replaced with a 1 ms floor so they cannot zero the denominator or skip work.
pub fn next_difficulty_weighted(
    config: &DaaConfig,
    parent_bits: Difficulty,
    samples: &[DaaSample],
) -> Difficulty {
    let parent = parent_bits.clamp(config);
    if samples.len() < 2 {
        return parent;
    }

    let mut total_work = 0u128;
    let mut elapsed_ms = 0u64;
    for window in samples.windows(2) {
        let older = window[0];
        let newer = window[1];
        let work_delta = newer
            .block_work
            .max(newer.blue_work.saturating_sub(older.blue_work))
            .max(1);
        total_work = total_work.saturating_add(work_delta);
        let dt = newer.timestamp_ms.saturating_sub(older.timestamp_ms).max(1);
        elapsed_ms = elapsed_ms.saturating_add(dt);
    }
    if total_work == 0 || elapsed_ms == 0 {
        return parent;
    }

    // observed_hashrate = total_window_work / robust_elapsed
    // next_target_work  = observed_hashrate × target_block_time
    // Map onto bits relative to the selected parent's own work contribution.
    let parent_work = work_from_bits(parent.level) as f64;
    if parent_work <= 0.0 {
        return parent;
    }
    let observed_hashrate = (total_work as f64) / (elapsed_ms as f64);
    let target = config.target_block_time_ms as f64;
    let next_target_work = observed_hashrate * target;
    let mut factor = next_target_work / parent_work;
    if !factor.is_finite() || factor <= 0.0 {
        return parent;
    }

    let max = config.max_adjustment_factor.max(1.0);
    if factor > max {
        factor = max;
    } else if factor < 1.0 / max {
        factor = 1.0 / max;
    }

    let delta_bits = factor.log2();
    let next = (parent.level as f64 + delta_bits).round() as i64;
    Difficulty {
        level: next.clamp(config.min_level as i64, config.max_level as i64) as u32,
    }
}

/// Uniform-work helper — prefer [`next_difficulty_weighted`] with real block work.
///
/// Synthesizes samples whose per-block work matches `work_from_bits(current)` so the
/// hashrate ratio is driven by timestamps alone.
pub fn next_difficulty(
    config: &DaaConfig,
    current: Difficulty,
    window_timestamps_ms: &[u64],
) -> Difficulty {
    let w = work_from_bits(current.clamp(config).as_bits());
    let samples: Vec<DaaSample> = window_timestamps_ms
        .iter()
        .enumerate()
        .map(|(i, &timestamp_ms)| DaaSample {
            timestamp_ms,
            blue_work: w.saturating_mul((i as u128).saturating_add(1)),
            block_work: w,
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
        let config = DaaConfig::default();
        let current = Difficulty { level: 8 };
        let ts: Vec<u64> = (0..91)
            .map(|i| i * (config.target_block_time_ms / 100).max(1))
            .collect();
        let next = next_difficulty(&config, current, &ts);
        assert!(
            next.level <= current.level + 1,
            "difficulty jumped from {} to {}",
            current.level,
            next.level
        );
        assert!(next.level >= current.level);
    }

    #[test]
    fn respects_max_level() {
        let config = DaaConfig {
            max_level: 12,
            max_adjustment_factor: 2.0,
            ..DaaConfig::default()
        };
        let current = Difficulty { level: 12 };
        let ts: Vec<u64> = (0..91)
            .map(|i| i * (config.target_block_time_ms / 100).max(1))
            .collect();
        let next = next_difficulty(&config, current, &ts);
        assert_eq!(next.level, 12);
    }

    #[test]
    fn can_rise_from_zero_when_too_fast() {
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
    fn work_from_bits_keeps_growing_past_63() {
        assert!(work_from_bits(64) > work_from_bits(63));
        assert_eq!(work_from_bits(127), 1u128 << 127);
        assert_eq!(work_from_bits(200), u128::MAX);
    }

    #[test]
    fn equal_timestamps_do_not_skip_work() {
        let config = DaaConfig {
            target_block_time_ms: 1_000,
            window_size: 2,
            max_adjustment_factor: 2.0,
            min_level: 1,
            max_level: 128,
        };
        let current = Difficulty { level: 10 };
        // Zero elapsed would previously skip the interval; 1 ms floor still counts work.
        let samples = vec![
            DaaSample {
                timestamp_ms: 1_000,
                blue_work: 100,
                block_work: 100,
            },
            DaaSample {
                timestamp_ms: 1_000,
                blue_work: 200,
                block_work: 100,
            },
            DaaSample {
                timestamp_ms: 1_000,
                blue_work: 300,
                block_work: 100,
            },
        ];
        let next = next_difficulty_weighted(&config, current, &samples);
        assert!(next.level >= current.level);
    }

    #[test]
    fn median_time_past_picks_middle() {
        let ts = [50, 10, 30, 20, 40];
        assert_eq!(median_time_past(&ts), 30);
    }

    #[test]
    fn fast_high_work_window_raises_difficulty() {
        let config = DaaConfig {
            target_block_time_ms: 1_000,
            window_size: 2,
            max_adjustment_factor: 4.0,
            min_level: 1,
            max_level: 128,
        };
        let current = Difficulty { level: 10 };
        let w = work_from_bits(20);
        let fast = vec![
            DaaSample {
                timestamp_ms: 0,
                blue_work: w,
                block_work: w,
            },
            DaaSample {
                timestamp_ms: 200,
                blue_work: 2 * w,
                block_work: w,
            },
            DaaSample {
                timestamp_ms: 400,
                blue_work: 3 * w,
                block_work: w,
            },
        ];
        let next = next_difficulty_weighted(&config, current, &fast);
        assert!(next.level > current.level, "got {}", next.level);
    }
}
