//! Minimal HTTP JSON-RPC client for `agora-node` template / submit loops.

use agora_rpc::{RpcRequest, RpcResponse};
use agora_types::{Block, Hash};
use serde_json::json;

pub async fn fetch_block_template(rpc_url: &str) -> Result<Block, String> {
    let resp = rpc_call(rpc_url, "agora_getBlockTemplate", json!([])).await?;
    let value = resp
        .result
        .ok_or_else(|| format!("template error: {:?}", resp.error))?;
    // Prefer `{ block, randomx_epoch }` wrapper; fall back to a bare Block for older nodes.
    if let Some(block) = value.get("block") {
        serde_json::from_value(block.clone()).map_err(|e| e.to_string())
    } else {
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

pub async fn submit_block(rpc_url: &str, block: &Block) -> Result<Hash, String> {
    let resp = rpc_call(rpc_url, "agora_submitBlock", json!(block)).await?;
    let value = resp
        .result
        .ok_or_else(|| format!("submit error: {:?}", resp.error))?;
    let id = value
        .get("block_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing block_id in {value}"))?;
    Hash::from_hex(id).ok_or_else(|| format!("invalid block_id hex `{id}`"))
}

async fn rpc_call(
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<RpcResponse, String> {
    let body = serde_json::to_string(&RpcRequest {
        id: Some(json!(1)),
        method: method.into(),
        params,
    })
    .map_err(|e| e.to_string())?;

    let host_port = url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("127.0.0.1:8545");
    let path = {
        let rest = url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        rest.find('/')
            .map(|i| rest[i..].to_string())
            .unwrap_or_else(|| "/rpc".into())
    };

    let mut stream = tokio::net::TcpStream::connect(host_port)
        .await
        .map_err(|e| format!("connect {host_port}: {e}"))?;
    let auth = std::env::var("AGORA_RPC_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf);
    let json_body = text
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| "missing HTTP body".to_string())?
        .trim();
    serde_json::from_str(json_body).map_err(|e| format!("rpc decode: {e}; body={json_body}"))
}
