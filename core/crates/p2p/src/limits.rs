//! Connection-count limits derived from `NetworkConfig::max_peers`.

use libp2p::connection_limits::{self, ConnectionLimits};

/// Build swarm connection limits from the configured peer cap.
///
/// - Total established connections ≤ `max_peers`
/// - Incoming established ≤ `max_peers` (paired with total to blunt eclipse risk)
/// - At most one established connection per peer
/// - Pending dials/accepts capped so bursts cannot overrun the swarm
pub fn connection_limits_for_max_peers(max_peers: u32) -> ConnectionLimits {
    let max = max_peers.max(1);
    let pending = max.saturating_mul(2).max(2);
    ConnectionLimits::default()
        .with_max_established(Some(max))
        .with_max_established_incoming(Some(max))
        .with_max_established_outgoing(Some(max))
        .with_max_established_per_peer(Some(1))
        .with_max_pending_incoming(Some(pending))
        .with_max_pending_outgoing(Some(pending))
}

pub fn connection_limits_behaviour(max_peers: u32) -> connection_limits::Behaviour {
    connection_limits::Behaviour::new(connection_limits_for_max_peers(max_peers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_nonzero_limits() {
        let limits = connection_limits_for_max_peers(0);
        // max(1) floor — Behaviour accepts these Options; we just ensure construction works.
        let _ = connection_limits::Behaviour::new(limits);
        let _ = connection_limits_behaviour(64);
    }
}
