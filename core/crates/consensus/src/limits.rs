//! Structural and monetary consensus limits for block / tx admission.

/// Caps shared by `agora-node` admission, templates, and mempool pre-checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsensusLimits {
    pub max_block_parents: usize,
    pub max_block_transactions: usize,
    pub max_tx_inputs: usize,
    pub max_tx_outputs: usize,
    pub max_tx_bytes: usize,
    /// Block-only cap while standalone DA mempool admission remains disabled.
    pub max_data_commitments: usize,
    pub max_block_bytes: usize,
    /// Reject headers with `timestamp_ms > now + this`.
    pub max_timestamp_ahead_ms: u64,
    /// Blue-score delta before a non-genesis coinbase output may be spent.
    pub coinbase_maturity: u64,
    /// Reject candidates whose parent blue-score lag behind the virtual tip exceeds this
    /// (bounds RandomX epoch thrashing / ancient-parent DoS).
    pub max_parent_blue_score_lag: u64,
}

impl Default for ConsensusLimits {
    fn default() -> Self {
        Self {
            max_block_parents: 16,
            // 1 coinbase + DEFAULT_TEMPLATE_TX_LIMIT (128)
            max_block_transactions: 129,
            max_tx_inputs: 64,
            max_tx_outputs: 64,
            max_tx_bytes: 100_000,
            max_data_commitments: 64,
            max_block_bytes: 1_000_000,
            max_timestamp_ahead_ms: 60_000,
            coinbase_maturity: 100,
            // ~2 RandomX epochs at 2048 blocks/epoch.
            max_parent_blue_score_lag: 4_096,
        }
    }
}

impl ConsensusLimits {
    pub const fn max_block_parents(&self) -> usize {
        self.max_block_parents
    }
}

/// Convenience aliases matching [`ConsensusLimits::default`].
pub const MAX_BLOCK_PARENTS: usize = 16;
pub const MAX_BLOCK_TRANSACTIONS: usize = 129;
pub const MAX_TX_INPUTS: usize = 64;
pub const MAX_TX_OUTPUTS: usize = 64;
pub const MAX_TX_BYTES: usize = 100_000;
pub const MAX_DATA_COMMITMENTS_PER_BLOCK: usize = 64;
pub const MAX_BLOCK_BYTES: usize = 1_000_000;
pub const MAX_TIMESTAMP_AHEAD_MS: u64 = 60_000;
pub const COINBASE_MATURITY: u64 = 100;
