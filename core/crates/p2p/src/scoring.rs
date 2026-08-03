//! Gossipsub mesh tuning + peer scoring for sub-second BlockDAG tips.
//!
//! Defaults favor faster heartbeats and tolerant small-mesh scoring so local
//! two-node tests and early testnets are not graylisted for low delivery counts.

use std::time::Duration;

use libp2p::gossipsub::{
    self, PeerScoreParams, PeerScoreThresholds, TopicScoreParams, ValidationMode,
};

use crate::topics::NetworkTopics;
use crate::P2pError;

/// Mesh / heartbeat knobs applied when building the gossipsub behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GossipTuning {
    /// Gossipsub heartbeat period (default 200ms for sub-second tips).
    pub heartbeat_interval: Duration,
    pub mesh_n: usize,
    pub mesh_n_low: usize,
    pub mesh_n_high: usize,
    pub mesh_outbound_min: usize,
    pub gossip_lazy: usize,
    pub history_length: usize,
    pub history_gossip: usize,
    pub duplicate_cache_time: Duration,
    /// Flood-publish to all topic peers (helpful for small / fast DAGs).
    pub flood_publish: bool,
    /// Enable gossipsub peer scoring + topic params.
    pub peer_scoring: bool,
}

impl Default for GossipTuning {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(200),
            // mesh_outbound_min <= mesh_n_low <= mesh_n <= mesh_n_high
            // mesh_outbound_min <= mesh_n / 2
            mesh_n: 6,
            mesh_n_low: 4,
            mesh_n_high: 12,
            mesh_outbound_min: 2,
            gossip_lazy: 4,
            history_length: 6,
            history_gossip: 3,
            duplicate_cache_time: Duration::from_secs(30),
            flood_publish: true,
            peer_scoring: true,
        }
    }
}

impl GossipTuning {
    /// Build a gossipsub [`Config`] with Agora mesh defaults.
    pub fn build_config(
        &self,
        message_id_fn: impl Fn(&gossipsub::Message) -> gossipsub::MessageId + Send + Sync + 'static,
    ) -> Result<gossipsub::Config, P2pError> {
        gossipsub::ConfigBuilder::default()
            .heartbeat_interval(self.heartbeat_interval)
            .validation_mode(ValidationMode::Strict)
            .mesh_n(self.mesh_n)
            .mesh_n_low(self.mesh_n_low)
            .mesh_n_high(self.mesh_n_high)
            .mesh_outbound_min(self.mesh_outbound_min)
            .gossip_lazy(self.gossip_lazy)
            .history_length(self.history_length)
            .history_gossip(self.history_gossip)
            .duplicate_cache_time(self.duplicate_cache_time)
            .flood_publish(self.flood_publish)
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|e| P2pError::Gossip(e.to_string()))
    }
}

/// Topic score params tolerant of tiny meshes (local / early testnet).
pub fn agora_topic_score_params(topic_weight: f64) -> TopicScoreParams {
    TopicScoreParams {
        topic_weight,
        // Stay grafted longer ⇒ modest positive score.
        time_in_mesh_weight: 0.01,
        time_in_mesh_quantum: Duration::from_secs(1),
        time_in_mesh_cap: 360.0,
        // Reward first deliveries.
        first_message_deliveries_weight: 1.0,
        first_message_deliveries_decay: score_decay(Duration::from_secs(10)),
        first_message_deliveries_cap: 100.0,
        // Soft P3: tiny meshes should not be graylisted.
        mesh_message_deliveries_weight: -0.1,
        mesh_message_deliveries_decay: score_decay(Duration::from_secs(10)),
        mesh_message_deliveries_cap: 20.0,
        mesh_message_deliveries_threshold: 1.0,
        mesh_message_deliveries_window: Duration::from_millis(50),
        mesh_message_deliveries_activation: Duration::from_secs(30),
        mesh_failure_penalty_weight: -0.1,
        mesh_failure_penalty_decay: score_decay(Duration::from_secs(60)),
        // Penalize invalid gossip payloads.
        invalid_message_deliveries_weight: -2.0,
        invalid_message_deliveries_decay: score_decay(Duration::from_secs(30)),
    }
}

fn score_decay(target: Duration) -> f64 {
    gossipsub::score_parameter_decay(target)
}

/// Peer score params with blocks weighted above txs for `topics`.
pub fn agora_peer_score_params(topics: &NetworkTopics) -> PeerScoreParams {
    let mut params = PeerScoreParams::default();
    params
        .topics
        .insert(topics.blocks().hash(), agora_topic_score_params(1.0));
    params
        .topics
        .insert(topics.transactions().hash(), agora_topic_score_params(0.5));
    // App-specific score (set via NetworkHandle) can reward good IBD peers.
    params.app_specific_weight = 1.0;
    // Soften IP colocation for local docker/dev (many peers on 127.0.0.1).
    params.ip_colocation_factor_weight = -0.5;
    params.ip_colocation_factor_threshold = 32.0;
    params.behaviour_penalty_weight = -5.0;
    params.behaviour_penalty_decay = score_decay(Duration::from_secs(60));
    params.retain_score = Duration::from_secs(600);
    params
}

pub fn agora_peer_score_thresholds() -> PeerScoreThresholds {
    PeerScoreThresholds {
        gossip_threshold: -10.0,
        publish_threshold: -50.0,
        graylist_threshold: -80.0,
        accept_px_threshold: 5.0,
        opportunistic_graft_threshold: 10.0,
    }
}

/// Activate peer scoring and attach Agora topic params for `topics`.
pub fn enable_peer_scoring(
    gossipsub: &mut gossipsub::Behaviour,
    topics: &NetworkTopics,
) -> Result<(), P2pError> {
    gossipsub
        .with_peer_score(
            agora_peer_score_params(topics),
            agora_peer_score_thresholds(),
        )
        .map_err(P2pError::Gossip)?;
    Ok(())
}

/// Positive app-score bump for a peer that delivered useful work (e.g. RR block).
pub const APP_SCORE_GOOD_PEER: f64 = 2.0;
/// Negative bump for peers that send unusable / rejectable payloads.
pub const APP_SCORE_BAD_PEER: f64 = -5.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tuning_builds_valid_gossip_config() {
        let tuning = GossipTuning::default();
        let cfg = tuning
            .build_config(|msg| gossipsub::MessageId::from(format!("t-{}", msg.data.len())))
            .expect("config");
        assert_eq!(cfg.mesh_n(), 6);
        assert_eq!(cfg.mesh_n_low(), 4);
        assert_eq!(cfg.heartbeat_interval(), Duration::from_millis(200));
        assert!(cfg.flood_publish());
    }

    #[test]
    fn peer_score_params_validate() {
        let topics = NetworkTopics::new("dev");
        agora_peer_score_params(&topics).validate().expect("params");
        agora_peer_score_thresholds()
            .validate()
            .expect("thresholds");
        agora_topic_score_params(1.0).validate().expect("topic");
    }
}
