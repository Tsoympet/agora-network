/// Physical RocksDB / store column families used by the state machine.
///
/// Roadmap calls for five CFs: three zones plus metadata and UTXO index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ColumnFamily {
    /// Tip hashes, mempool-adjacent caches, recent headers.
    Hot = 0,
    /// Queryable recent history for RPC / explorer (includes acceptance bitmaps).
    Warm = 1,
    /// Long-term archival payloads.
    Archival = 2,
    /// Chain meta: genesis hash, supply caps, tips set, network fingerprint.
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
    /// Full [`agora_types::NetworkFingerprint`] (borsh).
    pub const NETWORK_FINGERPRINT: &[u8] = b"meta/network_fingerprint";
}

/// Key helpers for acceptance + journal records in warm storage.
pub mod acceptance_keys {
    use agora_types::Hash;

    pub fn bitmap(block_hash: &Hash) -> Vec<u8> {
        let mut key = b"accept/bitmap/".to_vec();
        key.extend_from_slice(block_hash.as_bytes());
        key
    }

    pub fn journal(block_hash: &Hash) -> Vec<u8> {
        let mut key = b"accept/journal/".to_vec();
        key.extend_from_slice(block_hash.as_bytes());
        key
    }

    pub fn tx_index(tx_id: &Hash) -> Vec<u8> {
        let mut key = b"accept/tx/".to_vec();
        key.extend_from_slice(tx_id.as_bytes());
        key
    }
}

/// Encode an outpoint as a UTXO column key: `tx_id || index (le u32)`.
pub fn utxo_key(tx_id: &agora_types::Hash, index: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(36);
    key.extend_from_slice(tx_id.as_bytes());
    key.extend_from_slice(&index.to_le_bytes());
    key
}

pub fn utxo_key_outpoint(outpoint: &agora_types::OutPoint) -> Vec<u8> {
    utxo_key(&outpoint.tx_id, outpoint.index)
}
