//! Lightweight HTTP peer phonebook for Agora bootstrap.
//!
//! Endpoints:
//! - `GET /peers` — JSON array of multiaddrs
//! - `POST /peers` — body is a single multiaddr string to register
//! - `GET /health` — liveness
//!
//! When `AGORA_SEEDER_TOKEN` is set, `POST /peers` requires
//! `Authorization: Bearer <token>`.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let bind = std::env::var("AGORA_SEEDER_BIND").unwrap_or_else(|_| "127.0.0.1:18080".into());
    let token = std::env::var("AGORA_SEEDER_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(Arc::<str>::from);
    let peers: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    // Optional preload from env (comma-separated multiaddrs).
    if let Ok(preload) = std::env::var("AGORA_SEEDER_PEERS") {
        let mut guard = peers.lock().await;
        for peer in preload.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            guard.insert(peer.to_string());
        }
    }

    let listener = TcpListener::bind(&bind).await.expect("bind seeder");
    info!(
        %bind,
        post_auth = token.is_some(),
        "agora-dns-seeder listening"
    );

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "accept failed");
                continue;
            }
        };
        let peers = peers.clone();
        let token = token.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let response = handle_request(&req, &peers, token.as_deref()).await;
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(%addr, "served");
        });
    }
}

async fn handle_request(
    req: &str,
    peers: &Arc<Mutex<HashSet<String>>>,
    token: Option<&str>,
) -> String {
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("GET", "/health") => http_response(200, "ok"),
        ("GET", "/peers") => {
            let guard = peers.lock().await;
            let list: Vec<&String> = guard.iter().collect();
            let body = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
            http_response(200, &body)
        }
        ("POST", "/peers") => {
            if let Some(expected) = token {
                match extract_bearer_token(req) {
                    Some(provided) if token_matches(expected, provided) => {}
                    _ => return http_response(401, "unauthorized"),
                }
            }
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            if body.is_empty() {
                return http_response(400, "empty body");
            }
            // Accept raw multiaddr or JSON string.
            let peer = if let Ok(serde_json::Value::String(s)) =
                serde_json::from_str::<serde_json::Value>(body)
            {
                s
            } else {
                body.trim_matches('"').to_string()
            };
            peers.lock().await.insert(peer);
            http_response(200, "registered")
        }
        _ => http_response(404, "not found"),
    }
}

fn extract_bearer_token(req: &str) -> Option<&str> {
    for line in req.lines().skip(1) {
        if line.is_empty() || line == "\r" {
            break;
        }
        let line = line.trim_end_matches('\r');
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("authorization") {
            let value = value.trim();
            let rest = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))?;
            let token = rest.trim();
            if !token.is_empty() {
                return Some(token);
            }
        }
    }
    None
}

fn token_matches(expected: &str, provided: &str) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(provided.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_compare() {
        assert!(token_matches("abc", "abc"));
        assert!(!token_matches("abc", "Abc"));
    }
}
