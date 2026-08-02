//! Minimal HTTP JSON-RPC transport for `agora-node`.

use std::sync::Arc;

use agora_rpc::{RpcDispatcher, RpcRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::backend::NodeBackend;

/// Serve JSON-RPC over HTTP/1.1 (same style as dns-seeder / faucet).
///
/// - `GET /health` → `{"ok":true}`
/// - `POST /` or `POST /rpc` → body is [`RpcRequest`] JSON
pub async fn serve_rpc(
    bind: String,
    dispatcher: Arc<Mutex<RpcDispatcher<NodeBackend>>>,
) {
    let listener = TcpListener::bind(&bind).await.expect("bind rpc");
    info!(%bind, "agora-node JSON-RPC listening");

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "rpc accept failed");
                continue;
            }
        };
        let dispatcher = dispatcher.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let response = handle_request(&req, &dispatcher).await;
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(%addr, "rpc served");
        });
    }
}

async fn handle_request(
    req: &str,
    dispatcher: &Arc<Mutex<RpcDispatcher<NodeBackend>>>,
) -> String {
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("GET", "/health") => http_response(200, r#"{"ok":true}"#),
        ("POST", "/") | ("POST", "/rpc") => {
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            let parsed: RpcRequest = match serde_json::from_str(body) {
                Ok(v) => v,
                Err(err) => {
                    return http_response(
                        400,
                        &serde_json::json!({
                            "result": null,
                            "error": { "code": -32700, "message": format!("parse error: {err}") }
                        })
                        .to_string(),
                    );
                }
            };
            let mut guard = dispatcher.lock().await;
            let resp = guard.handle(parsed);
            match serde_json::to_string(&resp) {
                Ok(body) => http_response(200, &body),
                Err(err) => http_response(500, &format!(r#"{{"error":"{err}"}}"#)),
            }
        }
        _ => http_response(404, r#"{"error":"not found"}"#),
    }
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
