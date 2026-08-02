use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Minimal Stratum-style JSON-RPC line protocol for ASIC miners.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumRequest {
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumResponse {
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

impl StratumResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(serde_json::json!([code, message.into(), null])),
        }
    }
}
