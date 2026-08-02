use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use agora_types::Hash;

use crate::messages::BridgeMessage;
use crate::proof::{merkle_root, prove_message, verify_inclusion, LightClientProof};
use crate::BridgeError;

/// Production messaging surface between hub and District Chains.
pub trait MessageTransport: Send + Sync {
    fn publish(&self, district_id: &str, message: BridgeMessage) -> Result<Hash, BridgeError>;
    fn poll(&self, district_id: &str) -> Result<Option<BridgeMessage>, BridgeError>;
    fn commit_root(&self, district_id: &str) -> Result<Hash, BridgeError>;
    fn prove(&self, district_id: &str, message_id: Hash) -> Result<LightClientProof, BridgeError>;
    fn verify_against_root(
        &self,
        proof: &LightClientProof,
        expected_root: &Hash,
    ) -> Result<(), BridgeError>;
}

#[derive(Default)]
struct DistrictLane {
    queue: VecDeque<BridgeMessage>,
    committed: Vec<BridgeMessage>,
}

/// In-process transport used by tests and local multi-district sims.
#[derive(Clone, Default)]
pub struct InMemoryTransport {
    inner: Arc<Mutex<HashMap<String, DistrictLane>>>,
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_lane<R>(
        &self,
        district_id: &str,
        f: impl FnOnce(&mut DistrictLane) -> R,
    ) -> Result<R, BridgeError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| BridgeError::Transport("lock poisoned".into()))?;
        let lane = guard.entry(district_id.to_string()).or_default();
        Ok(f(lane))
    }
}

impl MessageTransport for InMemoryTransport {
    fn publish(&self, district_id: &str, message: BridgeMessage) -> Result<Hash, BridgeError> {
        let id = message.id();
        self.with_lane(district_id, |lane| {
            lane.queue.push_back(message);
        })?;
        Ok(id)
    }

    fn poll(&self, district_id: &str) -> Result<Option<BridgeMessage>, BridgeError> {
        self.with_lane(district_id, |lane| {
            let msg = lane.queue.pop_front();
            if let Some(ref m) = msg {
                lane.committed.push(m.clone());
            }
            msg
        })
    }

    fn commit_root(&self, district_id: &str) -> Result<Hash, BridgeError> {
        self.with_lane(district_id, |lane| {
            let leaves: Vec<Hash> = lane.committed.iter().map(BridgeMessage::id).collect();
            merkle_root(&leaves)
        })
    }

    fn prove(&self, district_id: &str, message_id: Hash) -> Result<LightClientProof, BridgeError> {
        self.with_lane(district_id, |lane| prove_message(&lane.committed, message_id))?
    }

    fn verify_against_root(
        &self,
        proof: &LightClientProof,
        expected_root: &Hash,
    ) -> Result<(), BridgeError> {
        if verify_inclusion(proof, expected_root) {
            Ok(())
        } else {
            Err(BridgeError::InvalidProof("inclusion check failed".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_types::{Address, Amount};

    use crate::messages::BridgeDirection;

    #[test]
    fn transport_publish_poll_prove() {
        let transport = InMemoryTransport::new();
        let msg = BridgeMessage {
            direction: BridgeDirection::LockAndMint,
            source_district: "agora".into(),
            dest_district: "arena".into(),
            sender: Address([1u8; 20]),
            recipient: Address([2u8; 20]),
            amount: Amount::from_base_units(10),
            nonce: 1,
        };
        let id = transport.publish("arena", msg.clone()).unwrap();
        assert_eq!(transport.poll("arena").unwrap().unwrap().id(), id);
        let root = transport.commit_root("arena").unwrap();
        let proof = transport.prove("arena", id).unwrap();
        transport.verify_against_root(&proof, &root).unwrap();
    }
}
