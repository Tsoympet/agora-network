use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agora_bridge_sdk::{BridgeBox, DistrictConfig};
use agora_types::Hash;

use crate::amm::ConstantProductPool;
use crate::intent::{Intent, IntentStatus, Solution};
use crate::IntentError;

/// Solver trait — AI / heuristic agents plug in here.
pub trait IntentSolver: Send + Sync {
    fn solve(&self, intent: &Intent, now_ms: u64) -> Result<Solution, IntentError>;
}

/// Naive single-hop solver: cross-district routes via hub at 1:1.
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
                "same-district intents need an AMM solver".into(),
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
            strategy: "bridge".into(),
        })
    }
}

/// Same-district constant-product AMM solver.
#[derive(Debug, Clone)]
pub struct AmmSolver {
    pub pool: Arc<Mutex<ConstantProductPool>>,
}

impl AmmSolver {
    pub fn new(pool: ConstantProductPool) -> Self {
        Self {
            pool: Arc::new(Mutex::new(pool)),
        }
    }

    pub fn shared_pool(&self) -> Arc<Mutex<ConstantProductPool>> {
        self.pool.clone()
    }
}

impl IntentSolver for AmmSolver {
    fn solve(&self, intent: &Intent, now_ms: u64) -> Result<Solution, IntentError> {
        if intent.is_expired(now_ms) {
            return Err(IntentError::Expired);
        }
        if intent.give_asset_district != intent.want_asset_district {
            return Err(IntentError::Unsolvable);
        }
        let pool = self
            .pool
            .lock()
            .map_err(|_| IntentError::Constraint("amm lock poisoned".into()))?;
        if pool.district_id != intent.give_asset_district {
            return Err(IntentError::Unsolvable);
        }
        let quoted = pool.quote(intent.give_amount)?;
        if quoted.as_base_units() < intent.min_receive.as_base_units() {
            return Err(IntentError::Unsolvable);
        }
        Ok(Solution {
            intent_id: intent.id(),
            receive_amount: quoted,
            route: vec![intent.give_asset_district.clone(), "amm".into()],
            strategy: "amm".into(),
        })
    }
}

/// Tries AMM for same-district, otherwise naive bridge route.
pub struct CompositeSolver {
    amm: Option<AmmSolver>,
    bridge: NaiveSolver,
}

impl CompositeSolver {
    pub fn new(amm: Option<AmmSolver>) -> Self {
        Self {
            amm,
            bridge: NaiveSolver,
        }
    }
}

impl IntentSolver for CompositeSolver {
    fn solve(&self, intent: &Intent, now_ms: u64) -> Result<Solution, IntentError> {
        if intent.give_asset_district == intent.want_asset_district {
            if let Some(amm) = &self.amm {
                return amm.solve(intent, now_ms);
            }
            return Err(IntentError::Constraint(
                "same-district intents need an AMM solver".into(),
            ));
        }
        let mut sol = self.bridge.solve(intent, now_ms)?;
        sol.strategy = "composite".into();
        Ok(sol)
    }
}

/// Intent-Engine orchestrates solvers and bridge settlement.
pub struct IntentEngine<S: IntentSolver> {
    solver: S,
    bridge: BridgeBox,
    intents: HashMap<Hash, (Intent, IntentStatus)>,
    amm_pool: Option<Arc<Mutex<ConstantProductPool>>>,
}

impl<S: IntentSolver> IntentEngine<S> {
    pub fn new(solver: S) -> Self {
        Self {
            solver,
            bridge: BridgeBox::new(),
            intents: HashMap::new(),
            amm_pool: None,
        }
    }

    pub fn with_bridge(mut self, bridge: BridgeBox) -> Self {
        self.bridge = bridge;
        self
    }

    pub fn with_amm_pool(mut self, pool: Arc<Mutex<ConstantProductPool>>) -> Self {
        self.amm_pool = Some(pool);
        self
    }

    pub fn register_district(&mut self, config: DistrictConfig) {
        self.bridge.register_district(config);
    }

    pub fn bridge(&self) -> &BridgeBox {
        &self.bridge
    }

    pub fn bridge_mut(&mut self) -> &mut BridgeBox {
        &mut self.bridge
    }

    pub fn submit(&mut self, intent: Intent, now_ms: u64) -> Result<Hash, IntentError> {
        if intent.is_expired(now_ms) {
            return Err(IntentError::Expired);
        }
        let id = intent.id();
        if self.intents.contains_key(&id) {
            return Err(IntentError::Constraint("duplicate intent".into()));
        }
        self.intents.insert(id, (intent, IntentStatus::Open));
        Ok(id)
    }

    pub fn cancel(&mut self, intent_id: Hash) -> Result<(), IntentError> {
        let entry = self
            .intents
            .get_mut(&intent_id)
            .ok_or(IntentError::Unknown)?;
        match entry.1 {
            IntentStatus::Open | IntentStatus::Routed => {
                entry.1 = IntentStatus::Cancelled;
                Ok(())
            }
            IntentStatus::Settled => Err(IntentError::AlreadySettled),
            IntentStatus::Cancelled => Err(IntentError::Cancelled),
            IntentStatus::Failed => Err(IntentError::Constraint("intent failed".into())),
        }
    }

    pub fn status(&self, id: &Hash) -> Option<IntentStatus> {
        self.intents.get(id).map(|(_, s)| *s)
    }

    pub fn get_intent(&self, id: &Hash) -> Option<&Intent> {
        self.intents.get(id).map(|(i, _)| i)
    }

    /// Solve and settle: bridge lock/mint+claim, or AMM swap for same-district.
    pub fn route_and_settle(
        &mut self,
        intent_id: Hash,
        now_ms: u64,
    ) -> Result<Solution, IntentError> {
        let (intent, status) = self
            .intents
            .get(&intent_id)
            .cloned()
            .ok_or(IntentError::Unknown)?;
        match status {
            IntentStatus::Settled => return Err(IntentError::AlreadySettled),
            IntentStatus::Cancelled => return Err(IntentError::Cancelled),
            IntentStatus::Failed => {
                return Err(IntentError::Constraint("intent failed".into()));
            }
            IntentStatus::Open | IntentStatus::Routed => {}
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

        if let Some(entry) = self.intents.get_mut(&intent_id) {
            entry.1 = IntentStatus::Routed;
        }

        if solution.strategy == "amm" || (intent.give_asset_district == intent.want_asset_district)
        {
            let pool = self
                .amm_pool
                .as_ref()
                .ok_or_else(|| IntentError::Constraint("no AMM pool configured".into()))?;
            let out = {
                let mut guard = pool
                    .lock()
                    .map_err(|_| IntentError::Constraint("amm lock poisoned".into()))?;
                let out = guard.apply_swap(intent.give_amount)?;
                if out.as_base_units() < intent.min_receive.as_base_units() {
                    return Err(IntentError::Constraint("amm slip below min_receive".into()));
                }
                out
            };
            // Bind AMM reserves to the DRC account ledger (pool = zero address).
            let pool_addr = agora_types::Address::ZERO;
            self.bridge
                .drc_mut()
                .transfer(
                    &intent.give_asset_district,
                    intent.user,
                    pool_addr,
                    intent.give_amount,
                    agora_types::Amount::ZERO,
                    None,
                )
                .map_err(|e| IntentError::Constraint(e.to_string()))?;
            self.bridge
                .drc_mut()
                .transfer(
                    &intent.want_asset_district,
                    pool_addr,
                    intent.user,
                    out,
                    agora_types::Amount::ZERO,
                    None,
                )
                .map_err(|e| IntentError::Constraint(e.to_string()))?;
        } else {
            // Balance-backed path payment via hub (XRP path-payment class).
            // Debits real source-district DRC — does not faucet-mint.
            let hub = solution
                .route
                .get(1)
                .cloned()
                .unwrap_or_else(|| "agora-hub".into());
            self.bridge
                .path_pay(
                    hub,
                    intent.give_asset_district.clone(),
                    intent.want_asset_district.clone(),
                    intent.user,
                    intent.user,
                    intent.give_amount,
                    intent.id_salt,
                    0,
                )
                .map_err(|e| IntentError::Constraint(e.to_string()))?;
        }

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
        let mut engine = IntentEngine::new(CompositeSolver::new(None));
        engine.register_district(DistrictConfig::general("agora-hub", 1));
        engine.register_district(DistrictConfig::gaming("arena", 1));
        engine.register_district(DistrictConfig::privacy("veil", 2));
        let user = Address([9u8; 20]);
        engine
            .bridge_mut()
            .drc_mut()
            .mint("arena", user, Amount::from_whole(5).unwrap())
            .unwrap();

        let intent = Intent {
            id_salt: 7,
            user,
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
        assert_eq!(
            engine.bridge().drc().balance("veil", user).as_base_units(),
            Amount::from_whole(5).unwrap().as_base_units()
        );
        assert_eq!(
            engine.bridge().drc().balance("arena", user).as_base_units(),
            0
        );
    }

    #[test]
    fn amm_same_district() {
        let amm = AmmSolver::new(ConstantProductPool::new("arena", 1_000_000, 1_000_000, 30));
        let pool_handle = amm.shared_pool();
        let mut engine =
            IntentEngine::new(CompositeSolver::new(Some(amm))).with_amm_pool(pool_handle);
        engine.register_district(DistrictConfig::gaming("arena", 1));
        let user = Address([3u8; 20]);
        let pool_addr = Address::ZERO;
        // Fund user + pool liquidity on the DRC ledger.
        engine
            .bridge_mut()
            .drc_mut()
            .mint("arena", user, Amount::from_base_units(10_000))
            .unwrap();
        engine
            .bridge_mut()
            .drc_mut()
            .mint("arena", pool_addr, Amount::from_base_units(1_000_000))
            .unwrap();

        let intent = Intent {
            id_salt: 1,
            user,
            give_asset_district: "arena".into(),
            give_amount: Amount::from_base_units(10_000),
            want_asset_district: "arena".into(),
            min_receive: Amount::from_base_units(1),
            deadline_ms: 10_000,
            solver_hint: "amm".into(),
        };
        let id = engine.submit(intent, 0).unwrap();
        let sol = engine.route_and_settle(id, 1).unwrap();
        assert_eq!(sol.strategy, "amm");
        assert!(sol.receive_amount.as_base_units() > 0);
        assert_eq!(engine.status(&id), Some(IntentStatus::Settled));
        assert_eq!(
            engine.bridge().drc().balance("arena", user).as_base_units(),
            sol.receive_amount.as_base_units()
        );
    }

    #[test]
    fn cancel_open_intent() {
        let mut engine = IntentEngine::new(NaiveSolver);
        let intent = Intent {
            id_salt: 2,
            user: Address([1u8; 20]),
            give_asset_district: "a".into(),
            give_amount: Amount::from_base_units(1),
            want_asset_district: "b".into(),
            min_receive: Amount::from_base_units(1),
            deadline_ms: 100,
            solver_hint: String::new(),
        };
        let id = engine.submit(intent, 0).unwrap();
        engine.cancel(id).unwrap();
        assert_eq!(engine.status(&id), Some(IntentStatus::Cancelled));
    }
}
