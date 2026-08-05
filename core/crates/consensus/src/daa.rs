/// Difficulty Adjustment Algorithm parameters for sub-second BlockDAG tips.
#[derive(Debug, Clone)]
pub struct DaaConfig {
    /// Target spacing between blocks in milliseconds.
    pub target_block_time_ms: u64,
    /// Sliding window length in blue-score units.
    pub window_size: u64,
    /// Clamp ratio when adjusting difficulty (e.g. 2.0 => at most 2x / 0.5x per window).
    pub max_adjustment_factor: f64,
}

impl Default for DaaConfig {
    fn default() -> Self {
        Self {
            target_block_time_ms: 1_000,
            window_size: 90,
            max_adjustment_factor: 2.0,
        }
    }
}

/// Compact `bits`-style difficulty (higher = harder). Phase 2 stores a simple integer target level.
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

    let observed = (last - first) as f64;
    let expected = config.target_block_time_ms as f64 * (window_timestamps_ms.len() as f64 - 1.0);
    let mut factor = expected / observed;
    let max = config.max_adjustment_factor;
    if factor > max {
        factor = max;
    } else if factor < 1.0 / max {
        factor = 1.0 / max;
    }

    let next = (current.level as f64 * factor).round() as u32;
    Difficulty { level: next.max(1) }
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
}
