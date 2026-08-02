//! RandomX CPU miner sidecar.
//!
//! Polls `agora-node` for block templates over HTTP JSON-RPC, searches nonces
//! with [`RandomXPowHasher`], and submits solutions via `agora_submitBlock`.
//!
//! Env:
//! - `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`)
//! - `AGORA_MINE_POLL_MS` (default `2000`)

use agora_consensus::{LeadingZeroPow, PowHasher, RandomXPowHasher};
use agora_rpc::{RpcRequest, RpcResponse};
use agora_types::Block;
use serde_json::json;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let rpc_url =
        std::env::var("AGORA_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545/rpc".into());
    let poll_ms = std::env::var("AGORA_MINE_POLL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000u64);

    info!(%rpc_url, poll_ms, "agora-miner RandomX sidecar starting");
    println!("agora-miner: RandomX loop → {rpc_url}");

    let hasher = RandomXPowHasher;
    let mut nonce_cursor = 0u64;

    loop {
        match mine_one_round(&rpc_url, &hasher, &mut nonce_cursor).await {
            Ok(Some(id)) => info!(block = %id, "submitted solution"),
            Ok(None) => {}
            Err(err) => warn!(error = %err, "mine round failed"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(poll_ms)).await;
    }
}

async fn mine_one_round(
    rpc_url: &str,
    hasher: &RandomXPowHasher,
    nonce_cursor: &mut u64,
) -> Result<Option<String>, String> {
    let template = rpc_call(rpc_url, "agora_getBlockTemplate", json!([])).await?;
    let mut block: Block = serde_json::from_value(
        template
            .result
            .ok_or_else(|| format!("template error: {:?}", template.error))?,
    )
    .map_err(|e| e.to_string())?;

    // Search a bounded nonce window per poll so we re-fetch fresh tips/timestamps.
    const WINDOW: u64 = 256;
    let start = *nonce_cursor;
    for n in start..start.saturating_add(WINDOW) {
        block.header.nonce = n;
        let digest = hasher.pow_hash(&block.header);
        if LeadingZeroPow::leading_zero_bits(&digest) >= block.header.bits {
            let submitted = rpc_call(rpc_url, "agora_submitBlock", json!(block.clone())).await?;
            *nonce_cursor = n.wrapping_add(1);
            let id = submitted
                .result
                .and_then(|v| v.get("block_id").cloned())
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| block.header.hash().to_hex());
            return Ok(Some(id));
        }
    }
    *nonce_cursor = start.wrapping_add(WINDOW);
    Ok(None)
}

async fn rpc_call(url: &str, method: &str, params: serde_json::Value) -> Result<RpcResponse, String> {
    let body = serde_json::to_string(&RpcRequest {
        id: Some(json!(1)),
        method: method.into(),
        params,
    })
    .map_err(|e| e.to_string())?;

    // Minimal HTTP/1.1 client (same style as infra services).
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
        let p = rest.find('/').map(|i| &rest[i..]).unwrap_or("/rpc");
        p.to_string()
    };

    let mut stream = tokio::net::TcpStream::connect(host_port)
        .await
        .map_err(|e| format!("connect {host_port}: {e}"))?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
