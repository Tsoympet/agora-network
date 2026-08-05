/// Physical RocksDB / store column families used by the state machine.
///
/// Roadmap calls for five CFs: three zones plus metadata and UTXO index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ColumnFamily {
    /// Tip hashes and recent full block bodies (prunable via hot_window).
    Hot = 0,
    /// Queryable recent history: tx index, utxo journals, durable `header/*`.
    Warm = 1,
    /// Long-term archival payloads.
    Archival = 2,
    /// Chain meta: genesis hash, supply caps, DAA cursor, tips set.
    Meta = 3,
    /// Spendable UTXO set keyed by outpoint.
    Utxo = 4,
}

impl ColumnFamily {
    pub const ALL: [Self; 5] = [
        Self::Hot,
        Self::Warm,
        Self::Archival,
        Self::Meta,
        Self::Utxo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Hot => "cf_hot",
            Self::Warm => "cf_warm",
            Self::Archival => "cf_archival",
            Self::Meta => "cf_meta",
            Self::Utxo => "cf_utxo",
        }
    }
}

/// Well-known meta keys (borsh / raw byte values).
pub mod meta_keys {
    pub const GENESIS_HASH: &[u8] = b"meta/genesis_hash";
    pub const MAX_SUPPLY: &[u8] = b"meta/max_supply";
    pub const PREMINE: &[u8] = b"meta/premine";
    pub const TIPS: &[u8] = b"meta/tips";
    /// Selected virtual tip (32-byte hash) — UTXO set follows blues of this tip.
    pub const VIRTUAL_TIP: &[u8] = b"meta/virtual_tip";
    /// Cumulative issued base units (premine + applied coinbase subsidies), `u64` LE.
    pub const ISSUED_SUPPLY: &[u8] = b"meta/issued_supply";
    /// Current DAA difficulty level (`u32` LE) — maps to `BlockHeader.bits`.
    pub const DAA_DIFFICULTY: &[u8] = b"meta/daa_difficulty";
    /// Civic governance + community board JSON snapshot ([`agora_governance::CivicSnapshot`]).
    pub const GOVERNANCE: &[u8] = b"meta/governance";
}
