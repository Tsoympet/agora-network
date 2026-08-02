use std::time::Duration;

use libp2p::identity::Keypair;

use crate::scoring::GossipTuning;

/// libp2p node configuration.
#[derive(Clone)]
pub struct NetworkConfig {
    /// Primary listen multiaddr (e.g. `/ip4/0.0.0.0/tcp/16111`).
    pub listen_addr: String,
    pub max_peers: u32,
    /// Optional bootstrap peers (multiaddrs that may include `/p2p/<peer_id>`).
    pub bootstrap: Vec<String>,
    /// Optional DNS seeder HTTP endpoint (e.g. `http://127.0.0.1:18080/peers`).
    pub dns_seeder_url: Option<String>,
    /// How often to re-fetch seeder peers and re-register (default 60s).
    /// Set to `Duration::ZERO` to disable periodic refresh.
    pub seeder_refresh_interval: Duration,
    /// Gossipsub mesh + peer-score tuning for sub-second DAG tips.
    pub gossip: GossipTuning,
    /// Persistent node identity. When `None`, [`crate::NetworkNode::build`] generates ephemeral keys.
    pub identity: Option<Keypair>,
}

impl std::fmt::Debug for NetworkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConfig")
            .field("listen_addr", &self.listen_addr)
            .field("max_peers", &self.max_peers)
            .field("bootstrap", &self.bootstrap)
            .field("dns_seeder_url", &self.dns_seeder_url)
            .field("seeder_refresh_interval", &self.seeder_refresh_interval)
            .field("gossip", &self.gossip)
            .field("identity", &self.identity.as_ref().map(|_| "<keypair>"))
            .finish()
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/16111".into(),
            max_peers: 64,
            bootstrap: Vec::new(),
            dns_seeder_url: None,
            seeder_refresh_interval: Duration::from_secs(60),
            gossip: GossipTuning::default(),
            identity: None,
        }
    }
}

impl NetworkConfig {
    pub fn with_listen(mut self, addr: impl Into<String>) -> Self {
        self.listen_addr = addr.into();
        self
    }

    pub fn with_bootstrap(mut self, peers: Vec<String>) -> Self {
        self.bootstrap = peers;
        self
    }

    pub fn with_dns_seeder(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        self.dns_seeder_url = if url.trim().is_empty() {
            None
        } else {
            Some(crate::normalize_seeder_url(&url))
        };
        self
    }

    pub fn with_max_peers(mut self, max_peers: u32) -> Self {
        self.max_peers = max_peers.max(1);
        self
    }

    pub fn with_gossip_tuning(mut self, gossip: GossipTuning) -> Self {
        self.gossip = gossip;
        self
    }

    pub fn with_seeder_refresh_interval(mut self, interval: Duration) -> Self {
        self.seeder_refresh_interval = interval;
        self
    }

    pub fn with_identity(mut self, identity: Keypair) -> Self {
        self.identity = Some(identity);
        self
    }
}
