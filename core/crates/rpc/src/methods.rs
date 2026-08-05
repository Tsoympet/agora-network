use serde::{Deserialize, Serialize};

use agora_types::{AcceptanceBitmap, Hash, TxAcceptanceStatus, TxConfirmation};

/// Canonical RPC method names (JSON-RPC style).
///
/// Transaction and confirmation queries are acceptance-aware: block color alone
/// is never sufficient to report a tx as confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethod {
    GetDagTips,
    GetBlock,
    SubmitTransaction,
    GetBalance,
    /// Acceptance bitmap for a blue block.
    GetBlockAcceptance,
    /// Confirmation status derived from acceptance (not blue inclusion alone).
    GetTxConfirmation,
}

impl RpcMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GetDagTips => "agora_getDagTips",
            Self::GetBlock => "agora_getBlock",
            Self::SubmitTransaction => "agora_submitTransaction",
            Self::GetBalance => "agora_getBalance",
            Self::GetBlockAcceptance => "agora_getBlockAcceptance",
            Self::GetTxConfirmation => "agora_getTxConfirmation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub result: serde_json::Value,
}

/// Explorer / wallet view of a block's transaction acceptance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAcceptanceView {
    pub block_hash: Hash,
    pub bitmap: AcceptanceBitmap,
    pub accepted_fees: u64,
    pub coinbase_reward: u64,
}

/// Explorer / wallet view of a transaction's acceptance status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxAcceptanceView {
    pub tx_id: Hash,
    pub status: TxAcceptanceStatus,
    pub confirmation: TxConfirmation,
}

impl TxAcceptanceView {
    pub fn from_confirmation(tx_id: Hash, confirmation: TxConfirmation) -> Self {
        Self {
            tx_id,
            status: confirmation.status,
            confirmation,
        }
    }
}
