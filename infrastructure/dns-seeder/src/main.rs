//! Lightweight HTTP peer phonebook for Agora bootstrap.
//!
//! Endpoints:
//! - `GET /peers` — JSON array of multiaddrs
//! - `POST /peers` — body is a single multiaddr string to register
//! - `GET /health` — liveness

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
    let peers: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    // Optional preload from env (comma-separated multiaddrs).
    if let Ok(preload) = std::env::var("AGORA_SEEDER_PEERS") {
        let mut guard = peers.lock().await;
        for peer in preload.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            guard.insert(peer.to_string());
        }
    }

    let listener = TcpListener::bind(&bind).await.expect("bind seeder");
    info!(%bind, "agora-dns-seeder listening");

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "accept failed");
                continue;
            }
        };
        let peers = peers.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let response = handle_request(&req, &peers).await;
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(%addr, "served");
        });
    }
}

async fn handle_request(req: &str, peers: &Arc<Mutex<HashSet<String>>>) -> String {
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
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            if body.is_empty() {
                return http_response(400, "empty body");
            }
            // Accept raw multiaddr or JSON string.
            let peer = if let Ok(serde_json::Value::String(s)) = serde_json::from_str::<serde_json::Value>(body)
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

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
