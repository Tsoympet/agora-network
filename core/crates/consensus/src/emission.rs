/// Fixed emission parameters for block rewards.
///
/// Centralizing the schedule prevents ad-hoc reward math in RPC or miner code.
#[derive(Debug, Clone)]
pub struct EmissionSchedule {
    pub initial_reward: u64,
    pub halving_interval: u64,
}

impl Default for EmissionSchedule {
    fn default() -> Self {
        Self {
            initial_reward: 50_0000_0000, // 50 AGORA with 8 decimals
            halving_interval: 210_000,
        }
    }
}

impl EmissionSchedule {
    pub fn reward_at_blue_score(&self, blue_score: u64) -> u64 {
        let halvings = blue_score / self.halving_interval;
        if halvings >= 64 {
            return 0;
        }
        self.initial_reward >> halvings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_on_interval() {
        let schedule = EmissionSchedule::default();
        assert_eq!(schedule.reward_at_blue_score(0), schedule.initial_reward);
        assert_eq!(
            schedule.reward_at_blue_score(schedule.halving_interval),
            schedule.initial_reward / 2
        );
    }
}
