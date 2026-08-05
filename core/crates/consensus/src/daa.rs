//! Difficulty Adjustment Algorithm for BlockDAG selected-parent spines.
//!
//! Expected bits for a candidate are derived from its **canonical selected parent**
//! context, never from whichever tip the local node retargeted most recently.
//!
//! Retarget math is **integer-only** (no `f64` / `log2`) so every architecture
//! produces bit-identical difficulty decisions from the same samples.

/// Difficulty Adjustment Algorithm parameters for sub-second BlockDAG tips.
#[derive(Debug, Clone)]
pub struct DaaConfig {
    /// Target spacing between blocks in milliseconds.
    pub target_block_time_ms: u64,
    /// Sliding window length in samples (timestamps), not including the newest alone.
    pub window_size: u64,
    /// Maximum absolute bit delta applied per window (e.g. `1` ⇒ at most 2× / 0.5× work).
    ///
    /// Retained `max_adjustment_factor` in genesis JSON maps onto this via
    /// [`Self::max_adjustment_bits_from_factor`].
    pub max_adjustment_bits: u32,
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
            max_adjustment_bits: 1,
            min_level: 1,
            max_level: 128,
        }
    }
}

impl DaaConfig {
    /// Map a genesis JSON adjustment factor (power-of-two style) onto bit deltas.
    ///
    /// `2.0 → 1`, `4.0 → 2`, values below 2 clamp to 1. Purely integer thresholds
    /// afterward — the factor itself is not used in sample math.
    pub fn max_adjustment_bits_from_factor(factor: f64) -> u32 {
        if factor >= 8.0 {
            3
        } else if factor >= 4.0 {
            2
        } else {
            1
        }
    }

    /// Compatibility accessor for genesis serialization.
    pub fn max_adjustment_factor(&self) -> f64 {
        match self.max_adjustment_bits {
            0 | 1 => 2.0,
            2 => 4.0,
            _ => 8.0,
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
/// Integer hashrate form:
/// `next_target_work ≈ total_window_work × target_block_time / elapsed`
/// compared against `parent_work` with exact doubling thresholds (no floats).
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

    let parent_work = work_from_bits(parent.level).max(1);
    let target = config.target_block_time_ms.max(1) as u128;
    let elapsed = elapsed_ms as u128;

    // Compare total_work * target  ?  parent_work * elapsed * 2^k without division.
    // num = total_work * target; den = parent_work * elapsed
    let num = total_work.saturating_mul(target);
    let den = parent_work.saturating_mul(elapsed);

    let max_bits = config.max_adjustment_bits.max(1);
    let mut delta: i64 = 0;
    if num >= den {
        // How many exact doublings of den fit under num?
        let mut thr = den;
        while delta < max_bits as i64 {
            let next_thr = thr.saturating_mul(2);
            if num >= next_thr {
                delta += 1;
                thr = next_thr;
                if thr == u128::MAX {
                    break;
                }
            } else {
                break;
            }
        }
    } else {
        // How many exact doublings of num fit under den?
        let mut thr = num;
        while delta > -(max_bits as i64) {
            let next_thr = thr.saturating_mul(2);
            if den >= next_thr {
                delta -= 1;
                thr = next_thr;
                if thr == u128::MAX {
                    break;
                }
            } else {
                break;
            }
        }
    }

    let next =
        (parent.level as i64 + delta).clamp(config.min_level as i64, config.max_level as i64);
    Difficulty { level: next as u32 }
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
            max_adjustment_bits: 1,
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
            max_adjustment_bits: 1,
            min_level: 1,
            max_level: 128,
        };
        let current = Difficulty { level: 10 };
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
            max_adjustment_bits: 2,
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

    #[test]
    fn integer_daa_is_deterministic() {
        let config = DaaConfig::default();
        let current = Difficulty { level: 10 };
        let ts: Vec<u64> = (0..91).map(|i| i * 500).collect();
        let a = next_difficulty(&config, current, &ts);
        let b = next_difficulty(&config, current, &ts);
        assert_eq!(a, b);
    }
}
