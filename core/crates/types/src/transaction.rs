use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Hash;

/// Bech32-ready raw payload for a secp256k1-derived address (20-byte hash of pubkey).
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct Address(pub [u8; 20]);

/// Single transaction output.
#[derive(
    Clone, PartialEq, Eq, Debug,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct TxOut {
    pub value: u64,
    pub address: Address,
}

/// Minimal transfer transaction used across consensus, mempool, and RPC.
///
/// Signature bytes are opaque here so `agora-crypto` owns verification policy.
#[derive(
    Clone, PartialEq, Eq, Debug,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<Hash>,
    pub outputs: Vec<TxOut>,
    pub nonce: u64,
    pub signature: Vec<u8>,
}
