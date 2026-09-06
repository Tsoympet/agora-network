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

/// Explicit datadir schema version (u32 LE in [`meta_keys::SCHEMA_VERSION`]).
///
/// - `1` — pre-Trident L1 UTXO (genesis v2)
/// - `2` — Trident acceptance records + per-asset supply keys + OVL/DRC accounts
/// - `3` — staking + finality Meta keys
/// - `4` — state-root commitments + staking reserve remaining + signed stake ops
/// - `5` — multi-lane block body (account/stake) + working non-zero staking reserves
/// - `6` — signed OVL execution lane + full multi-lane acceptance commitment
/// - `7` — native DRC payment lane, duplicate/invoice index, and outbox
/// - `8` — canonical governance policy and asset-isolated protocol treasuries
/// - `9` — canonical Hub, Passport, Grant, and Mission registry summary
/// - `10` — authenticated DA commitment/index/replay state and revert journal
pub const SCHEMA_VERSION: u32 = 10;

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
    /// In-flight virtual reorg target (32-byte hash). Cleared after the UTXO
    /// transition + virtual-tip meta commit atomically. Present across a crash means
    /// recovery should finish (or redo) the reorg toward this tip.
    pub const PENDING_VIRTUAL: &[u8] = b"meta/pending_virtual";
    /// Datadir schema version (`u32` LE). Missing key ⇒ treat as schema 1.
    pub const SCHEMA_VERSION: &[u8] = b"meta/schema_version";
    /// Per-asset issued supply prefix: `meta/issued_supply/<asset_wire_byte>`.
    pub const ISSUED_SUPPLY_ASSET_PREFIX: &[u8] = b"meta/issued_supply/";

    // These keys reserve a lossless candidate Block 0 record and its bound
    // datadir identity for a future atomic loader. The v2 ignition path never
    // writes them, and their independent versions do not advance the live
    // datadir schema.
    pub const TRIDENT_BLOCK_ZERO_RECORD_VERSION: &[u8] = b"meta/trident_block_zero/record_version";
    pub const TRIDENT_BLOCK_ZERO_RECORD: &[u8] = b"meta/trident_block_zero/record";
    pub const TRIDENT_BLOCK_ZERO_STATE_PAYLOAD: &[u8] = b"meta/trident_block_zero/state_payload";
    pub const TRIDENT_BLOCK_ZERO_STATE_ROOT: &[u8] = b"meta/trident_block_zero/state_root";
    pub const TRIDENT_BLOCK_ZERO_COMMITMENT: &[u8] = b"meta/trident_block_zero/commitment";
    pub const TRIDENT_BLOCK_ZERO_COMMITMENT_HASH: &[u8] =
        b"meta/trident_block_zero/commitment_hash";
    pub const TRIDENT_BLOCK_ZERO_ARTIFACT_IDENTITY: &[u8] =
        b"meta/trident_block_zero/artifact_identity";
    pub const TRIDENT_BLOCK_ZERO_CONSENSUS_POLICY_HASH: &[u8] =
        b"meta/trident_block_zero/consensus_policy_hash";
    pub const TRIDENT_BLOCK_ZERO_NETWORK_FINGERPRINT: &[u8] =
        b"meta/trident_block_zero/network_fingerprint";
    pub const TRIDENT_BLOCK_ZERO_CHAIN_ID: &[u8] = b"meta/trident_block_zero/chain_id";

    pub const TRIDENT_DATADIR_IDENTITY_VERSION: &[u8] = b"meta/trident_datadir_identity/version";
    pub const TRIDENT_DATADIR_IDENTITY: &[u8] = b"meta/trident_datadir_identity/record";
}
