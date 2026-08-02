use std::collections::HashMap;

use agora_bridge_sdk::{BridgeBox, DistrictConfig};
use agora_types::Hash;

use crate::intent::{Intent, IntentStatus, Solution};
use crate::IntentError;

/// Solver trait — AI / heuristic agents plug in here.
pub trait IntentSolver: Send + Sync {
    fn solve(&self, intent: &Intent, now_ms: u64) -> Result<Solution, IntentError>;
}

/// Naive single-hop solver: same-district swap is unsupported; cross-district
/// routes via hub if min_receive can be met at 1:1 for the scaffold.
#[derive(Debug, Default)]
pub struct NaiveSolver;

impl IntentSolver for NaiveSolver {
    fn solve(&self, intent: &Intent, now_ms: u64) -> Result<Solution, IntentError> {
        if intent.is_expired(now_ms) {
            return Err(IntentError::Expired);
        }
        if intent.give_amount.as_base_units() < intent.min_receive.as_base_units() {
            return Err(IntentError::Unsolvable);
        }
        if intent.give_asset_district == intent.want_asset_district {
            return Err(IntentError::Constraint(
                "same-district intents need a local AMM adapter".into(),
            ));
        }
        Ok(Solution {
            intent_id: intent.id(),
            receive_amount: intent.min_receive,
            route: vec![
                intent.give_asset_district.clone(),
                "agora-hub".into(),
                intent.want_asset_district.clone(),
            ],
        })
    }
}

/// Intent-Engine orchestrates solvers and optional bridge settlement.
pub struct IntentEngine<S: IntentSolver> {
    solver: S,
    bridge: BridgeBox,
    intents: HashMap<Hash, (Intent, IntentStatus)>,
}

impl<S: IntentSolver> IntentEngine<S> {
    pub fn new(solver: S) -> Self {
        Self {
            solver,
            bridge: BridgeBox::new(),
            intents: HashMap::new(),
        }
    }

    pub fn register_district(&mut self, config: DistrictConfig) {
        self.bridge.register_district(config);
    }

    pub fn bridge(&self) -> &BridgeBox {
        &self.bridge
    }

    pub fn submit(&mut self, intent: Intent, now_ms: u64) -> Result<Hash, IntentError> {
        if intent.is_expired(now_ms) {
            return Err(IntentError::Expired);
        }
        let id = intent.id();
        self.intents.insert(id, (intent, IntentStatus::Open));
        Ok(id)
    }

    pub fn status(&self, id: &Hash) -> Option<IntentStatus> {
        self.intents.get(id).map(|(_, s)| *s)
    }

    /// Solve and settle via Bridge-in-a-Box lock/mint for the scaffold path.
    pub fn route_and_settle(&mut self, intent_id: Hash, now_ms: u64) -> Result<Solution, IntentError> {
        let (intent, status) = self
            .intents
            .get(&intent_id)
            .cloned()
            .ok_or(IntentError::Unsolvable)?;
        if status == IntentStatus::Settled {
            return Err(IntentError::AlreadySettled);
        }
        if intent.is_expired(now_ms) {
            if let Some(entry) = self.intents.get_mut(&intent_id) {
                entry.1 = IntentStatus::Failed;
            }
            return Err(IntentError::Expired);
        }

        let solution = self.solver.solve(&intent, now_ms)?;
        if solution.receive_amount.as_base_units() < intent.min_receive.as_base_units() {
            return Err(IntentError::Constraint("min_receive not met".into()));
        }

        // Scaffold settlement: lock on source district hub lane, mint toward want district.
        self.bridge
            .lock_and_mint(
                intent.give_asset_district.clone(),
                intent.want_asset_district.clone(),
                intent.user,
                intent.user,
                intent.give_amount,
                intent.id_salt,
            )
            .map_err(|e| IntentError::Constraint(e.to_string()))?;

        if let Some(entry) = self.intents.get_mut(&intent_id) {
            entry.1 = IntentStatus::Settled;
        }
        Ok(solution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount};

    #[test]
    fn routes_cross_district_intent() {
        let mut engine = IntentEngine::new(NaiveSolver);
        engine.register_district(DistrictConfig::gaming("arena", 1));
        engine.register_district(DistrictConfig::privacy("veil", 2));

        let intent = Intent {
            id_salt: 7,
            user: Address([9u8; 20]),
            give_asset_district: "arena".into(),
            give_amount: Amount::from_whole(5).unwrap(),
            want_asset_district: "veil".into(),
            min_receive: Amount::from_whole(5).unwrap(),
            deadline_ms: 10_000,
            solver_hint: "naive".into(),
        };
        let id = engine.submit(intent, 0).unwrap();
        let solution = engine.route_and_settle(id, 100).unwrap();
        assert_eq!(solution.route.len(), 3);
        assert_eq!(engine.status(&id), Some(IntentStatus::Settled));
    }
}
