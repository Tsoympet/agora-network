use std::sync::Arc;

use agora_bridge_sdk::{
    BridgeBox, DistrictConfig, DrachmaGenesis, DrcBlock, InMemoryTransport, MessageStatus,
    DRACHMA_POW_ALGORITHM,
};
use agora_intent_engine::{
    AmmSolver, CompositeSolver, ConstantProductPool, Intent, IntentEngine, IntentStatus, Solution,
};
use agora_ovolos_rollup::{
    Batch, BatchCommitment, BatchStatus, EvmExecutor, EvmTx, FraudProof, OvlBlock, OvolosGenesis,
    OvolosRollup, RevmExecutor, OVOLOS_POW_ALGORITHM,
};
use agora_types::{Address, Amount, Hash};
use serde::Serialize;

use crate::LayersError;

#[derive(Debug, Clone)]
pub struct LayersRuntimeConfig {
    pub challenge_window_ms: Option<u64>,
    pub gas_payer: Option<Address>,
    pub hub_id: Option<String>,
    pub ovolos_genesis: OvolosGenesis,
    pub drachma_genesis: DrachmaGenesis,
}

impl Default for LayersRuntimeConfig {
    fn default() -> Self {
        Self {
            challenge_window_ms: None,
            gas_payer: None,
            hub_id: None,
            ovolos_genesis: OvolosGenesis::testnet(),
            drachma_genesis: DrachmaGenesis::testnet(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LayerInfo {
    pub hub_id: String,
    pub ovl_genesis_hash: String,
    pub ovl_chain_id: String,
    pub ovl_native: bool,
    pub ovl_pow_algorithm: String,
    pub ovl_pow_bits: u32,
    pub ovl_tip_height: u64,
    pub ovl_tip_hash: String,
    pub drc_genesis_hash: String,
    pub drc_chain_id: String,
    pub drc_native: bool,
    pub drc_pow_algorithm: String,
    pub drc_pow_bits: u32,
    pub rollup_head: String,
    pub next_sequence: u64,
    pub challenge_window_ms: u64,
    pub ovl_minted: u64,
    pub ovl_max_supply: u64,
    pub drc_minted: u64,
    pub drc_max_supply: u64,
    pub districts: Vec<String>,
    pub open_intents: usize,
    /// Hybrid: bonded sequencers gate OVL batch submit/finalize.
    pub ovl_hybrid: bool,
    pub ovl_active_sequencers: usize,
    /// Hybrid: bonded attestors quorum-finalize DRC messages.
    pub drc_hybrid: bool,
    pub drc_active_attestors: usize,
    pub drc_quorum_threshold: usize,
}

/// In-process L2 + L3 + L4 stack for operators and integration tests.
///
/// L2 uses `RevmExecutor` so OVL behaves as Ethereum-class gas + EVM state.
pub struct LayersRuntime {
    rollup: OvolosRollup<RevmExecutor>,
    intents: IntentEngine<CompositeSolver>,
    transport: Arc<InMemoryTransport>,
    /// Pending compact EVM txs (`to||value||data`) for Ethereum-class mempool.
    l2_mempool: Vec<EvmTx>,
    ovl_genesis_hash: String,
    ovl_chain_id: String,
    /// Numeric chain id for `eth_chainId` (derived from genesis string hash).
    ovl_eth_chain_id: u64,
    drc_genesis_hash: String,
    drc_chain_id: String,
    hub_id: String,
}

impl LayersRuntime {
    pub fn new(config: LayersRuntimeConfig) -> Result<Self, LayersError> {
        let ovl_genesis = config.ovolos_genesis.clone();
        let drc_genesis = config.drachma_genesis.clone();
        let hub_id = config
            .hub_id
            .clone()
            .unwrap_or_else(|| drc_genesis.hub_id.clone());

        let mut rollup =
            OvolosRollup::from_genesis(&ovl_genesis, RevmExecutor::default(), config.gas_payer)
                .map_err(|e| LayersError::Rollup(e.to_string()))?;
        // Operator override for local soak tests — does not rewrite frozen genesis_hash.
        if let Some(ms) = config.challenge_window_ms {
            rollup.config_mut().challenge_window_ms = ms;
        }

        let transport = Arc::new(InMemoryTransport::new());
        let bridge = BridgeBox::from_genesis(&drc_genesis)
            .map_err(|e| LayersError::Bridge(e.to_string()))?
            .with_transport(transport.clone());

        let pool = ConstantProductPool::new("arena", 5_000_000, 5_000_000, 30);
        let amm = AmmSolver::new(pool);
        let shared = amm.shared_pool();
        let intents = IntentEngine::new(CompositeSolver::new(Some(amm)))
            .with_bridge(bridge)
            .with_amm_pool(shared);

        // Stable numeric chain id for eth_* wallets (not L1 chain id).
        let ovl_eth_chain_id = {
            let bytes = ovl_genesis.genesis_hash.as_bytes();
            let mut n = 888_802u64; // Agora L2 namespace prefix
            for b in bytes.iter().take(8) {
                n = n.wrapping_mul(31).wrapping_add(*b as u64);
            }
            n
        };

        Ok(Self {
            ovl_genesis_hash: ovl_genesis.genesis_hash.clone(),
            ovl_chain_id: ovl_genesis.chain_id.clone(),
            ovl_eth_chain_id,
            drc_genesis_hash: drc_genesis.genesis_hash.clone(),
            drc_chain_id: drc_genesis.chain_id.clone(),
            hub_id,
            rollup,
            intents,
            transport,
            l2_mempool: Vec::new(),
        })
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
            hub_id: self.hub_id.clone(),
            ovl_genesis_hash: self.ovl_genesis_hash.clone(),
            ovl_chain_id: self.ovl_chain_id.clone(),
            ovl_native: true,
            ovl_pow_algorithm: OVOLOS_POW_ALGORITHM.into(),
            ovl_pow_bits: self.rollup.pow_bits(),
            ovl_tip_height: self.rollup.tip_height(),
            ovl_tip_hash: self.rollup.tip_hash().to_hex(),
            drc_genesis_hash: self.drc_genesis_hash.clone(),
            drc_chain_id: self.drc_chain_id.clone(),
            drc_native: true,
            drc_pow_algorithm: DRACHMA_POW_ALGORITHM.into(),
            drc_pow_bits: self.intents.bridge().pow_bits(),
            rollup_head: self.rollup.head_state_root().to_hex(),
            next_sequence: self.rollup.next_sequence(),
            challenge_window_ms: self.rollup.config().challenge_window_ms,
            ovl_minted: self.rollup.ovl().minted(),
            ovl_max_supply: self.rollup.ovl().max_supply(),
            drc_minted: self.intents.bridge().drc().minted(),
            drc_max_supply: self.intents.bridge().drc().max_supply(),
            districts,
            open_intents: 0,
            ovl_hybrid: self.rollup.sequencers().authorization_required(),
            ovl_active_sequencers: self.rollup.sequencers().active_sequencers().len(),
            drc_hybrid: self.intents.bridge().attestors().finality_required(),
            drc_active_attestors: self.intents.bridge().attestors().active_attestors().len(),
            drc_quorum_threshold: self.intents.bridge().attestors().quorum_threshold(),
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

    pub fn submit_batch_as(
        &mut self,
        sequencer: Address,
        batch: Batch,
    ) -> Result<Hash, LayersError> {
        self.rollup
            .submit_batch_as(sequencer, batch)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn bond_sequencer(
        &mut self,
        sequencer: Address,
        amount: Amount,
    ) -> Result<u64, LayersError> {
        self.rollup
            .bond_sequencer(sequencer, amount)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    pub fn finalize_due_as(
        &mut self,
        sequencer: Address,
        now_ms: u64,
    ) -> Result<Vec<Hash>, LayersError> {
        self.rollup
            .finalize_due_as(sequencer, now_ms)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    /// Execute txs against the current rollup head and return the post-state root.
    pub fn execute_evm_batch(
        &self,
        prev_state_root: &Hash,
        txs: &[EvmTx],
    ) -> Result<Hash, LayersError> {
        self.rollup
            .executor()
            .apply_batch(prev_state_root, txs)
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

    pub fn bond_attestor(&mut self, attestor: Address, amount: Amount) -> Result<u64, LayersError> {
        self.intents
            .bridge_mut()
            .bond_attestor(attestor, amount)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn attest_message(
        &mut self,
        attestor: Address,
        message_id: Hash,
    ) -> Result<bool, LayersError> {
        self.intents
            .bridge_mut()
            .attest_message(attestor, message_id)
            .map_err(|e| LayersError::Bridge(e.to_string()))
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

    /// Mine and admit a native OVL PoW block sealing `batch` (coinbase to `miner`).
    pub fn mine_ovl_block(
        &mut self,
        batch: Batch,
        miner: Address,
        timestamp_ms: u64,
        max_nonces: u64,
    ) -> Result<OvlBlock, LayersError> {
        self.rollup
            .mine_and_admit(batch, miner, timestamp_ms, max_nonces)
            .map_err(|e| LayersError::Rollup(e.to_string()))
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
        self.lock_and_mint_tagged(
            source_hub,
            dest_district,
            sender,
            recipient,
            amount,
            nonce,
            0,
        )
    }

    pub fn lock_and_mint_tagged(
        &mut self,
        source_hub: &str,
        dest_district: &str,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
    ) -> Result<Hash, LayersError> {
        self.intents
            .bridge_mut()
            .lock_and_mint_tagged(
                source_hub,
                dest_district,
                sender,
                recipient,
                amount,
                nonce,
                destination_tag,
            )
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

    /// Same-district DRC payment (XRP Payment–class) with destination tag.
    pub fn pay_drc(
        &mut self,
        district: &str,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
    ) -> Result<Hash, LayersError> {
        self.intents
            .bridge_mut()
            .pay(district, sender, recipient, amount, nonce, destination_tag)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    /// Cross-district path payment via hub (XRP path-payment class).
    pub fn path_pay_drc(
        &mut self,
        hub: &str,
        source_district: &str,
        dest_district: &str,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
    ) -> Result<(Hash, Hash), LayersError> {
        self.path_pay_drc_deliver(
            hub,
            source_district,
            dest_district,
            sender,
            recipient,
            amount,
            nonce,
            destination_tag,
            Amount::ZERO,
        )
    }

    /// Path payment with XRPL-class deliverMin.
    pub fn path_pay_drc_deliver(
        &mut self,
        hub: &str,
        source_district: &str,
        dest_district: &str,
        sender: Address,
        recipient: Address,
        amount: Amount,
        nonce: u64,
        destination_tag: u32,
        deliver_min: Amount,
    ) -> Result<(Hash, Hash), LayersError> {
        self.intents
            .bridge_mut()
            .path_pay_deliver(
                hub,
                source_district,
                dest_district,
                sender,
                recipient,
                amount,
                nonce,
                destination_tag,
                deliver_min,
            )
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    /// Mine and admit a native DRC PoW block on a district/hub (coinbase to `miner`).
    pub fn mine_drc_block(
        &mut self,
        district_id: &str,
        message_ids: Vec<Hash>,
        miner: Address,
        timestamp_ms: u64,
        max_nonces: u64,
    ) -> Result<DrcBlock, LayersError> {
        self.intents
            .bridge_mut()
            .mine_and_admit(district_id, message_ids, miner, timestamp_ms, max_nonces)
            .map_err(|e| LayersError::Bridge(e.to_string()))
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

    pub fn finalize_intent(&mut self, id: Hash) -> Result<(), LayersError> {
        self.intents
            .finalize_intent(id)
            .map_err(|e| LayersError::Intent(e.to_string()))
    }

    pub fn intent_status(&self, id: &Hash) -> Option<IntentStatus> {
        self.intents.status(id)
    }

    pub fn register_destination_tag(
        &mut self,
        district: &str,
        owner: Address,
        tag: u32,
    ) -> Result<(), LayersError> {
        self.intents
            .bridge_mut()
            .register_destination_tag(district, owner, tag)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn payments_for_tag(&self, district: &str, tag: u32) -> Vec<Hash> {
        self.intents.bridge().payments_for_tag(district, tag)
    }

    pub fn destination_tag_owner(&self, district: &str, tag: u32) -> Option<Address> {
        self.intents.bridge().destination_tag_owner(district, tag)
    }

    pub fn unbond_attestor(
        &mut self,
        attestor: Address,
        amount: Amount,
    ) -> Result<u64, LayersError> {
        self.intents
            .bridge_mut()
            .unbond_attestor(attestor, amount)
            .map_err(|e| LayersError::Bridge(e.to_string()))
    }

    pub fn unbond_sequencer(
        &mut self,
        sequencer: Address,
        amount: Amount,
    ) -> Result<u64, LayersError> {
        self.rollup
            .unbond_sequencer(sequencer, amount)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    // --- Ethereum-class L2 helpers (`eth_*`) ---

    pub fn eth_chain_id(&self) -> u64 {
        self.ovl_eth_chain_id
    }

    pub fn eth_block_number(&self) -> u64 {
        self.rollup.tip_height().max(self.rollup.next_sequence())
    }

    /// Prefer OVL ledger balance (native gas money); fall back to EVM account wei.
    pub fn eth_get_balance(&self, address: Address) -> u128 {
        let ovl = self.rollup.ovl().balance(address).as_base_units() as u128;
        if ovl > 0 {
            return ovl;
        }
        self.rollup
            .executor()
            .balance_of(&self.rollup.head_state_root(), address.0)
            .unwrap_or(0)
    }

    pub fn eth_get_transaction_count(&self, address: Address) -> u64 {
        self.rollup
            .executor()
            .nonce_of(&self.rollup.head_state_root(), address.0)
            .unwrap_or(0)
    }

    pub fn eth_get_code(&self, address: Address) -> Vec<u8> {
        self.rollup
            .executor()
            .code_of(&self.rollup.head_state_root(), address.0)
    }

    pub fn eth_get_storage_at(&self, address: Address, slot: [u8; 32]) -> [u8; 32] {
        self.rollup
            .executor()
            .storage_at_bytes(&self.rollup.head_state_root(), address.0, slot)
    }

    pub fn eth_call(&self, to: Address, data: &[u8], value: u128) -> Result<Vec<u8>, LayersError> {
        self.rollup
            .executor()
            .eth_call(&self.rollup.head_state_root(), to.0, data, value)
            .map_err(|e| LayersError::Rollup(e.to_string()))
    }

    /// Admit a compact EVM tx into the L2 mempool (`eth_sendRawTransaction`).
    pub fn eth_send_raw_transaction(&mut self, raw: EvmTx) -> Result<Hash, LayersError> {
        if raw.0.len() < 52 {
            return Err(LayersError::Rollup(
                "evm tx too short; expected to||value[||data]".into(),
            ));
        }
        let id = Hash::hash_bytes(&raw.0);
        if self.l2_mempool.iter().any(|t| Hash::hash_bytes(&t.0) == id) {
            return Ok(id);
        }
        self.l2_mempool.push(raw);
        Ok(id)
    }

    pub fn l2_mempool_len(&self) -> usize {
        self.l2_mempool.len()
    }

    /// Drain pending L2 txs (for sequencer batch building).
    pub fn drain_l2_mempool(&mut self) -> Vec<EvmTx> {
        std::mem::take(&mut self.l2_mempool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_ovolos_rollup::encode_value_transfer;

    #[test]
    fn end_to_end_layers_from_genesis() {
        let payer = Address([0xA1; 20]);
        let mut rt = LayersRuntime::new(LayersRuntimeConfig {
            challenge_window_ms: Some(100),
            gas_payer: Some(payer),
            hub_id: None,
            ovolos_genesis: OvolosGenesis::testnet(),
            drachma_genesis: DrachmaGenesis::testnet(),
        })
        .unwrap();

        // Genesis already preminted OVL to treasury; mint gas for payer separately.
        rt.mint_ovl(payer, Amount::from_base_units(1_000_000))
            .unwrap();

        let to = [0xB2u8; 20];
        let txs = vec![encode_value_transfer(to, 42)];
        let post = rt.execute_evm_batch(&Hash::ZERO, &txs).unwrap();
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
        assert_eq!(rt.finalize_due(100).unwrap(), vec![batch_id]);

        let treasury = Address::from_hex("ff9ec96f09eb154d038a552ecae59c50204ea9a9").unwrap();
        assert!(rt.drc_balance("agora-hub", treasury).as_base_units() > 0);
        assert!(rt.ovl_balance(treasury).as_base_units() > 0);
        assert!(rt.eth_chain_id() > 0);
        // gas_payer was charged flat OVL gas for the batch.
        assert!(rt.eth_get_balance(payer) < 1_000_000);
        assert!(rt.eth_get_balance(payer) > 0);

        let info = rt.info();
        assert_eq!(info.ovl_chain_id, "agora-ovolos-testnet-1");
        assert_eq!(info.drc_chain_id, "agora-drachma-testnet-1");
        assert!(info.ovl_native);
        assert!(info.drc_native);
        assert_eq!(info.ovl_pow_algorithm, OVOLOS_POW_ALGORITHM);
        assert_eq!(info.drc_pow_algorithm, DRACHMA_POW_ALGORITHM);
        assert!(!info.ovl_genesis_hash.is_empty());
        assert!(!info.drc_genesis_hash.is_empty());
    }
}
