use agora_types::NetworkFingerprint;

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
    /// Network fingerprint that binds gossip topics and mempool admission.
    pub fingerprint: NetworkFingerprint,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/16111".into(),
            max_peers: 64,
            bootstrap: Vec::new(),
            dns_seeder_url: None,
            fingerprint: NetworkFingerprint {
                network_name: "agora-devnet".into(),
                network_id: 1,
                genesis_hash: agora_types::Hash::ZERO,
                ghostdag_k: 18,
                max_supply: 0,
                premine: 0,
                initial_reward: 0,
                halving_interval: 210_000,
            },
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

    pub fn with_fingerprint(mut self, fingerprint: NetworkFingerprint) -> Self {
        self.fingerprint = fingerprint;
        self
    }
}
