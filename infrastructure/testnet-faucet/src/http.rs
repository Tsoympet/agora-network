use std::sync::Arc;

use agora_types::Address;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::faucet::FaucetService;

#[derive(Debug, Deserialize)]
struct DripBody {
    address: String,
}

/// Serve faucet over a tiny HTTP surface (same style as dns-seeder).
///
/// - `GET /health`
/// - `POST /drip` JSON `{ "address": "<40-hex>" }`
/// - `GET /balance/<40-hex>`
pub async fn serve(bind: &str, faucet: Arc<Mutex<FaucetService>>) {
    let listener = TcpListener::bind(bind).await.expect("bind faucet");
    info!(%bind, "agora-testnet-faucet listening");

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "accept failed");
                continue;
            }
        };
        let faucet = faucet.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 16_384];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let response = handle_request(&req, &faucet).await;
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(%addr, "served");
        });
    }
}

async fn handle_request(req: &str, faucet: &Arc<Mutex<FaucetService>>) -> String {
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("GET", "/health") => http_response(200, r#"{"ok":true}"#),
        ("POST", "/drip") => {
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            let parsed: DripBody = match serde_json::from_str(body) {
                Ok(v) => v,
                Err(err) => {
                    return http_response(400, &format!(r#"{{"error":"bad json: {err}"}}"#));
                }
            };
            let address = match Address::from_hex(&parsed.address) {
                Some(a) => a,
                None => {
                    return http_response(400, r#"{"error":"invalid address"}"#);
                }
            };
            let mut guard = faucet.lock().await;
            match guard.drip(address) {
                Ok(bal) => http_response(
                    200,
                    &serde_json::json!({
                        "address": address.to_hex(),
                        "balance": bal.as_base_units(),
                    })
                    .to_string(),
                ),
                Err(err) => {
                    let code = match err {
                        crate::FaucetError::RateLimited(_) => 429,
                        crate::FaucetError::Exhausted => 503,
                        _ => 400,
                    };
                    http_response(code, &format!(r#"{{"error":"{err}"}}"#))
                }
            }
        }
        (m, p) if m == "GET" && p.starts_with("/balance/") => {
            let hex = p.trim_start_matches("/balance/");
            let address = match Address::from_hex(hex) {
                Some(a) => a,
                None => return http_response(400, r#"{"error":"invalid address"}"#),
            };
            let guard = faucet.lock().await;
            let bal = guard.balance(&address);
            http_response(
                200,
                &serde_json::json!({
                    "address": address.to_hex(),
                    "balance": bal.as_base_units(),
                })
                .to_string(),
            )
        }
        _ => http_response(404, r#"{"error":"not found"}"#),
    }
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
