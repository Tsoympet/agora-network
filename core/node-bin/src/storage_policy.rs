//! Durable storage retention: archival history vs hot tip window.

/// Node retention policy for block payloads in `cf_hot` / `cf_archival`.
///
/// - Meta, UTXO, and Warm tx-index are never pruned by this policy.
/// - When `archival` is false, block bodies older than `hot_window` tip-distance
///   are deleted from Hot and are not written to Archival (pruned node).
///
/// **Consensus caveat:** full `blue_order` UTXO validation needs the bodies of
/// every newly accepted merge-set blue. Until finalized pruning points and state
/// snapshots exist, non-archival nodes can reject merges that archival peers
/// accept when a side-block body has aged out of `hot_window`. Default remains
/// archival; pruned mode is best-effort only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoragePolicy {
    /// Persist full block bodies in `cf_archival`.
    pub archival: bool,
    /// Tip-distance of bodies retained in `cf_hot`. `0` keeps Hot unlimited.
    pub hot_window: u32,
}

impl Default for StoragePolicy {
    fn default() -> Self {
        Self {
            archival: true,
            // Keep enough bodies for merge-set validation / reorgs within the
            // relay parent-age window (see ConsensusLimits::max_parent_blue_score_lag).
            hot_window: 4_096,
        }
    }
}

impl StoragePolicy {
    pub fn from_env() -> Self {
        let archival = !matches!(
            std::env::var("AGORA_ARCHIVAL").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE") | Ok("no") | Ok("off")
        );
        let hot_window = std::env::var("AGORA_HOT_WINDOW")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4_096);
        Self {
            archival,
            hot_window,
        }
    }
}
