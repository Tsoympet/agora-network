use serde::{Deserialize, Serialize};

/// Canonical RPC method names (JSON-RPC style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethod {
    GetDagTips,
    GetBlock,
    SubmitTransaction,
    GetBalance,
}

impl RpcMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GetDagTips => "agora_getDagTips",
            Self::GetBlock => "agora_getBlock",
            Self::SubmitTransaction => "agora_submitTransaction",
            Self::GetBalance => "agora_getBalance",
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
