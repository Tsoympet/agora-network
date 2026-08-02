/// libp2p node configuration.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Primary listen multiaddr (e.g. `/ip4/0.0.0.0/tcp/16111`).
    pub listen_addr: String,
    pub max_peers: u32,
    /// Optional bootstrap peers (multiaddrs that may include `/p2p/<peer_id>`).
    pub bootstrap: Vec<String>,
    /// Optional DNS seeder HTTP endpoint (e.g. `http://127.0.0.1:18080/peers`).
    pub dns_seeder_url: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/16111".into(),
            max_peers: 64,
            bootstrap: Vec::new(),
            dns_seeder_url: None,
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
}
