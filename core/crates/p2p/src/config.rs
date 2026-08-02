/// libp2p node configuration (listen addresses filled in Phase 4).
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub listen_addr: String,
    pub max_peers: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/16111".into(),
            max_peers: 64,
        }
    }
}
