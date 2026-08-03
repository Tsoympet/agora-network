//! Durable storage retention: archival history vs hot tip window.

/// Node retention policy for block payloads in `cf_hot` / `cf_archival`.
///
/// - Meta, UTXO, and Warm tx-index are never pruned by this policy.
/// - When `archival` is false, block bodies older than `hot_window` tip-distance
///   are deleted from Hot and are not written to Archival (pruned node).
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
            hot_window: 64,
        }
    }
}

impl StoragePolicy {
    pub fn from_env() -> Self {
        let archival = match std::env::var("AGORA_ARCHIVAL").as_deref() {
            Ok("0") | Ok("false") | Ok("FALSE") | Ok("no") | Ok("off") => false,
            _ => true,
        };
        let hot_window = std::env::var("AGORA_HOT_WINDOW")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(64);
        Self {
            archival,
            hot_window,
        }
    }
}
