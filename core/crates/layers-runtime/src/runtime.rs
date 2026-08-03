use std::sync::Arc;

use agora_bridge_sdk::{BridgeBox, DistrictConfig, InMemoryTransport, MessageStatus};
use agora_intent_engine::{
    AmmSolver, CompositeSolver, ConstantProductPool, Intent, IntentEngine, IntentStatus, Solution,
};
use agora_ovolos_rollup::{
    Batch, BatchCommitment, BatchStatus, FraudProof, OvolosRollup, RollupConfig, StubEvmExecutor,
};
use agora_types::{Address, Amount, Hash};
use serde::Serialize;

use crate::LayersError;

#[derive(Debug, Clone)]
pub struct LayersRuntimeConfig {
    pub challenge_window_ms: u64,
    pub gas_payer: Option<Address>,
    pub hub_id: String,
}

impl Default for LayersRuntimeConfig {
    fn default() -> Self {
        Self {
            challenge_window_ms: 60_000,
            gas_payer: None,
            hub_id: "agora-hub".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LayerInfo {
    pub hub_id: String,
    pub rollup_head: String,
    pub next_sequence: u64,
    pub challenge_window_ms: u64,
    pub ovl_minted: u64,
    pub ovl_max_supply: u64,
    pub drc_minted: u64,
    pub drc_max_supply: u64,
    pub districts: Vec<String>,
    pub open_intents: usize,
}

/// In-process L2 + L3 + L4 stack for operators and integration tests.
pub struct LayersRuntime {
    config: LayersRuntimeConfig,
    rollup: OvolosRollup<StubEvmExecutor>,
    intents: IntentEngine<CompositeSolver>,
    transport: Arc<InMemoryTransport>,
}

impl LayersRuntime {
    pub fn new(config: LayersRuntimeConfig) -> Self {
        let gas_payer = config.gas_payer;
        let rollup = OvolosRollup::new(
            RollupConfig {
                challenge_window_ms: config.challenge_window_ms,
                gas_payer,
            },
            StubEvmExecutor,
            Hash::ZERO,
        );
        let transport = Arc::new(InMemoryTransport::new());
        let bridge = BridgeBox::new().with_transport(transport.clone());
        let pool = ConstantProductPool::new("arena", 5_000_000, 5_000_000, 30);
        let amm = AmmSolver::new(pool);
        let shared = amm.shared_pool();
        let intents = IntentEngine::new(CompositeSolver::new(Some(amm)))
            .with_bridge(bridge)
            .with_amm_pool(shared);

        let mut rt = Self {
            config,
            rollup,
            intents,
            transport,
        };
        // Default districts for local demos.
        rt.register_district(DistrictConfig::gaming("arena", 9001));
        rt.register_district(DistrictConfig::privacy("veil", 9002));
        rt.register_district(DistrictConfig::general("agora-hub", 1));
        rt
    }

    pub fn register_district(&mut self, config: DistrictConfig) {
        self.intents.register_district(config);
    }

    pub fn transport(&self) -> Arc<InMemoryTransport> {
        self.transport.clone()
    }

    pub fn info(&self) -> LayerInfo {
        let districts: Vec<String> = self
            .intents
            .bridge()
            .districts()
            .map(|d| d.district_id.clone())
            .collect();
        LayerInfo {
            hub_id: self.config.hub_id.clone(),
            rollup_head: self.rollup.head_state_root().to_hex(),
            next_sequence: self.rollup.next_sequence(),
            challenge_window_ms: self.config.challenge_window_ms,
            ovl_minted: self.rollup.ovl().minted(),
            ovl_max_supply: self.rollup.ovl().max_supply(),
            drc_minted: self.intents.bridge().drc().minted(),
            drc_max_supply: self.intents.bridge().drc().max_supply(),
            districts,
            open_intents: 0,
        }
    }

    // --- L2 ---

    pub fn mint_ovl(&mut self, to: Address, amount: Amount) -> Result<(), LayersError> {
        self.rollup
            .ovl_mut()
            .mint(to, amount)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn submit_batch(&mut self, batch: Batch) -> Result<Hash, LayersError> {
        self.rollup
            .submit_batch(batch)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn record_da(&mut self, commitment: BatchCommitment) -> Result<(), LayersError> {
        self.rollup
            .record_da_post(commitment)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn challenge(&mut self, proof: FraudProof) -> Result<(), LayersError> {
        self.rollup
            .challenge(proof)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn finalize_due(&mut self, now_ms: u64) -> Result<Vec<Hash>, LayersError> {
        self.rollup
            .finalize_due(now_ms)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn batch_status(&self, id: &Hash) -> Option<BatchStatus> {
        self.rollup.batch_status(id)
    }

    pub fn get_commitment(&self, id: &Hash) -> Option<BatchCommitment> {
        self.rollup.get_commitment(id).cloned()
    }

    pub fn ovl_balance(&self, address: Address) -> Amount {
        self.rollup.ovl().balance(address)
    }

    // --- L3 ---

    pub fn credit_drc(
        &mut self,
        hub: &str,
        address: Address,
        amount: Amount,
    ) -> Result<(), LayersError> {
        self.intents
            .bridge_mut()
            .credit_hub_lock(hub, address, amount)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn lock_and_mint(
        &mut self,
        source_hub: &str,
        dest_district: &str,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
    ) -> Result<Hash, LayersError> {
        self.intents
            .bridge_mut()
            .lock_and_mint(source_hub, dest_district, sender, recipient, amount, nonce)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn claim_mint(&mut self, message_id: Hash) -> Result<(), LayersError> {
        self.intents
            .bridge_mut()
            .claim_mint(message_id)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn burn_and_unlock(
        &mut self,
        source_district: &str,
        dest_hub: &str,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
    ) -> Result<Hash, LayersError> {
        self.intents
            .bridge_mut()
            .burn_and_unlock(source_district, dest_hub, sender, recipient, amount, nonce)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn message_status(&self, id: &Hash) -> Option<MessageStatus> {
        self.intents.bridge().message_status(id)
    }

    pub fn drc_balance(&self, district: &str, address: Address) -> Amount {
        self.intents.bridge().drc().balance(district, address)
    }

    // --- L4 ---

    pub fn submit_intent(&mut self, intent: Intent, now_ms: u64) -> Result<Hash, LayersError> {
        self.intents
            .submit(intent, now_ms)
            .map_err(|e| LayersError::Intent(e.to_string()))
    }

    pub fn settle_intent(&mut self, id: Hash, now_ms: u64) -> Result<Solution, LayersError> {
        self.intents
            .route_and_settle(id, now_ms)
            .map_err(|e| LayersError::Intent(e.to_string()))
    }

    pub fn cancel_intent(&mut self, id: Hash) -> Result<(), LayersError> {
        self.intents
            .cancel(id)
            .map_err(|e| LayersError::Intent(e.to_string()))
    }

    pub fn intent_status(&self, id: &Hash) -> Option<IntentStatus> {
        self.intents.status(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_ovolos_rollup::{EvmExecutor, EvmTx, StubEvmExecutor};

    #[test]
    fn end_to_end_layers() {
        let payer = Address([0xA1; 20]);
        let mut rt = LayersRuntime::new(LayersRuntimeConfig {
            challenge_window_ms: 100,
            gas_payer: Some(payer),
            hub_id: "agora-hub".into(),
        });
        rt.mint_ovl(payer, Amount::from_base_units(1_000_000))
            .unwrap();

        let stub = StubEvmExecutor;
        let txs = vec![EvmTx(vec![1, 2, 3])];
        let post = stub.apply_batch(&Hash::ZERO, &txs).unwrap();
        let batch = Batch {
            sequence: 0,
            prev_state_root: Hash::ZERO,
            post_state_root: post,
            transactions: txs,
            posted_at_ms: 0,
        };
        let batch_id = rt.submit_batch(batch).unwrap();
        let commitment = rt.get_commitment(&batch_id).unwrap();
        rt.record_da(commitment).unwrap();
        let finalized = rt.finalize_due(100).unwrap();
        assert_eq!(finalized, vec![batch_id]);

        let alice = Address([1u8; 20]);
        let bob = Address([2u8; 20]);
        let amt = Amount::from_whole(3).unwrap();
        rt.credit_drc("agora-hub", alice, amt).unwrap();
        let mid = rt
            .lock_and_mint("agora-hub", "arena", alice, bob, amt, 1)
            .unwrap();
        rt.claim_mint(mid).unwrap();
        assert_eq!(rt.drc_balance("arena", bob), amt);

        let intent = Intent {
            id_salt: 9,
            user: Address([9u8; 20]),
            give_asset_district: "arena".into(),
            give_amount: Amount::from_whole(1).unwrap(),
            want_asset_district: "veil".into(),
            min_receive: Amount::from_whole(1).unwrap(),
            deadline_ms: 50_000,
            solver_hint: "composite".into(),
        };
        let iid = rt.submit_intent(intent, 0).unwrap();
        let sol = rt.settle_intent(iid, 10).unwrap();
        assert!(!sol.route.is_empty());
        assert_eq!(rt.intent_status(&iid), Some(IntentStatus::Settled));

        let info = rt.info();
        assert!(info.districts.iter().any(|d| d == "arena"));
        assert!(info.ovl_minted > 0);
    }
}
