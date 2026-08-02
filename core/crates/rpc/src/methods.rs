use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::RpcError;

/// Canonical RPC method names (JSON-RPC style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethod {
    GetDagTips,
    GetBlock,
    SubmitTransaction,
    GetBalance,
    GetUtxos,
    FundAddress,
    GetBlockTemplate,
    SubmitBlock,
}

impl RpcMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GetDagTips => "agora_getDagTips",
            Self::GetBlock => "agora_getBlock",
            Self::SubmitTransaction => "agora_submitTransaction",
            Self::GetBalance => "agora_getBalance",
            Self::GetUtxos => "agora_getUtxos",
            Self::FundAddress => "agora_fundAddress",
            Self::GetBlockTemplate => "agora_getBlockTemplate",
            Self::SubmitBlock => "agora_submitBlock",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "agora_getDagTips" => Some(Self::GetDagTips),
            "agora_getBlock" => Some(Self::GetBlock),
            "agora_submitTransaction" => Some(Self::SubmitTransaction),
            "agora_getBalance" => Some(Self::GetBalance),
            "agora_getUtxos" => Some(Self::GetUtxos),
            "agora_fundAddress" => Some(Self::FundAddress),
            "agora_getBlockTemplate" => Some(Self::GetBlockTemplate),
            "agora_submitBlock" => Some(Self::SubmitBlock),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorBody {
    pub code: i64,
    pub message: String,
}

impl RpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Value>, err: &RpcError) -> Self {
        Self {
            id,
            result: None,
            error: Some(RpcErrorBody {
                code: err.code(),
                message: err.to_string(),
            }),
        }
    }
}
