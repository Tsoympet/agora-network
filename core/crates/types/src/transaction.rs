use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Amount, Hash};

/// Bech32-ready raw payload for a secp256k1-derived address (20-byte hash of pubkey).
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub const ZERO: Self = Self([0u8; 20]);

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).ok()?;
        if bytes.len() != 20 {
            return None;
        }
        let mut out = [0u8; 20];
        out.copy_from_slice(&bytes);
        Some(Self(out))
    }
}

/// Reference to a previous transaction output spent by an input.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, Debug, Default,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct OutPoint {
    pub tx_id: Hash,
    pub index: u32,
}

/// Single transaction input.
#[derive(
    Clone, PartialEq, Eq, Debug,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct TxIn {
    pub previous_outpoint: OutPoint,
}

/// Single transaction output.
#[derive(
    Clone, PartialEq, Eq, Debug,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct TxOut {
    pub value: Amount,
    pub address: Address,
}

/// Fields covered by the secp256k1 signature (excludes auth material).
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
pub struct TransactionBody {
    pub version: u32,
    pub inputs: Vec<TxIn>,
    pub outputs: Vec<TxOut>,
    pub nonce: u64,
}

/// Transfer transaction used across consensus, mempool, and RPC.
#[derive(
    Clone, PartialEq, Eq, Debug,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<TxIn>,
    pub outputs: Vec<TxOut>,
    pub nonce: u64,
    /// Compressed secp256k1 public key (33 bytes) once signed; empty when unsigned.
    pub public_key: Vec<u8>,
    /// Compact ECDSA signature (64 bytes) once signed; empty when unsigned.
    pub signature: Vec<u8>,
}

impl Transaction {
    pub fn body(&self) -> TransactionBody {
        TransactionBody {
            version: self.version,
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            nonce: self.nonce,
        }
    }

    /// Canonical bytes that wallets sign / verifiers check.
    pub fn signing_bytes(&self) -> Vec<u8> {
        borsh::to_vec(&self.body()).expect("borsh serialize is infallible for TransactionBody")
    }

    /// Transaction ID is the hash of the full signed (or unsigned) encoding.
    pub fn tx_id(&self) -> Hash {
        Hash::hash_borsh(self)
    }

    pub fn unsigned(version: u32, inputs: Vec<TxIn>, outputs: Vec<TxOut>, nonce: u64) -> Self {
        Self {
            version,
            inputs,
            outputs,
            nonce,
            public_key: Vec::new(),
            signature: Vec::new(),
        }
    }
}
