//! Minimal HTTP JSON-RPC transport for `agora-node`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agora_rpc::{RpcDispatcher, RpcRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::backend::NodeBackend;

/// Optional bearer token + bind policy for the HTTP JSON-RPC server.
#[derive(Clone, Debug, Default)]
pub struct RpcHttpConfig {
    /// When set, mutating / wallet RPC methods require `Authorization: Bearer …`.
    pub token: Option<Arc<str>>,
    /// Max POST /rpc requests per peer IP per rolling minute (`0` disables).
    pub rate_limit_per_minute: u32,
}

#[derive(Default)]
struct RateLimiter {
    /// peer IP → (window start, count)
    windows: HashMap<IpAddr, (Instant, u32)>,
}

impl RateLimiter {
    fn allow(&mut self, ip: IpAddr, limit: u32) -> bool {
        if limit == 0 {
            return true;
        }
        let now = Instant::now();
        let entry = self.windows.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) >= Duration::from_secs(60) {
            *entry = (now, 0);
        }
        if entry.1 >= limit {
            return false;
        }
        entry.1 += 1;
        true
    }
}

/// Serve JSON-RPC over HTTP/1.1 (same style as dns-seeder / faucet).
///
/// - `GET /health` → `{"ok":true}` (always unauthenticated)
/// - `POST /` or `POST /rpc` → body is [`RpcRequest`] JSON
pub async fn serve_rpc(
    bind: String,
    dispatcher: Arc<Mutex<RpcDispatcher<NodeBackend>>>,
    config: RpcHttpConfig,
) {
    let listener = TcpListener::bind(&bind).await.expect("bind rpc");
    let limiter = Arc::new(Mutex::new(RateLimiter::default()));
    info!(
        %bind,
        token_required = config.token.is_some(),
        rate_limit_per_minute = config.rate_limit_per_minute,
        "agora-node JSON-RPC listening"
    );

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "rpc accept failed");
                continue;
            }
        };
        let dispatcher = dispatcher.clone();
        let config = config.clone();
        let limiter = limiter.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.lines().next().unwrap_or("").starts_with("POST") {
                let mut guard = limiter.lock().await;
                if !guard.allow(addr.ip(), config.rate_limit_per_minute) {
                    let response = http_response(
                        429,
                        &serde_json::json!({
                            "result": null,
                            "error": {
                                "code": -32029,
                                "message": "rate limited: too many RPC requests"
                            }
                        })
                        .to_string(),
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    return;
                }
            }
            let response = handle_request(&req, &dispatcher, &config).await;
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(%addr, "rpc served");
        });
    }
}

/// Returns `true` when `bind` resolves to a loopback host (IPv4/IPv6).
pub fn is_loopback_bind(bind: &str) -> bool {
    let host = bind.rsplit_once(':').map(|(h, _)| h).unwrap_or(bind);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "0:0:0:0:0:0:0:1")
}

/// True when a non-loopback RPC bind is allowed (`AGORA_RPC_ALLOW_PUBLIC_BIND=1`).
pub fn env_allows_public_rpc_bind() -> bool {
    matches!(
        std::env::var("AGORA_RPC_ALLOW_PUBLIC_BIND").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

/// Abort-friendly check: refuse public binds unless explicitly allowed.
pub fn enforce_rpc_bind_policy(bind: &str, token_set: bool) {
    if is_loopback_bind(bind) {
        return;
    }
    if !env_allows_public_rpc_bind() {
        panic!(
            "AGORA_RPC_BIND={bind} is not loopback; set AGORA_RPC_ALLOW_PUBLIC_BIND=1 to acknowledge public exposure"
        );
    }
    if !token_set {
        warn!(
            %bind,
            "public RPC bind without AGORA_RPC_TOKEN — unauthenticated JSON-RPC on a non-loopback address"
        );
    } else {
        warn!(%bind, "RPC bound on non-loopback address (AGORA_RPC_ALLOW_PUBLIC_BIND)");
    }
}

/// Read-only methods that stay public when `AGORA_RPC_TOKEN` is configured
/// (explorer tip sync / hydrate). Wallet writes and mining control require the token.
pub fn method_requires_token(method: &str) -> bool {
    !matches!(
        method,
        "agora_getDagTips"
            | "agora_getBlock"
            | "agora_getTransaction"
            | "agora_getMempool"
            | "agora_getNodeInfo"
            | "agora_estimateFee"
            | "agora_getConstitution"
            | "agora_getGovernance"
            | "agora_listProposals"
            | "agora_getProposal"
            | "agora_listOffices"
            | "agora_listForumTopics"
            | "agora_getFinality"
            | "agora_getFinalizedTip"
            | "agora_getValidatorSet"
            | "agora_getValidator"
            | "agora_getRewardPool"
            | "agora_getProtocolTreasuries"
    )
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
    // Length-mismatch short-circuit; equal length uses byte XOR fold.
    if expected.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(provided.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn unauthorized_response() -> String {
    http_response(
        401,
        &serde_json::json!({
            "result": null,
            "error": {
                "code": -32001,
                "message": "unauthorized: set Authorization: Bearer <AGORA_RPC_TOKEN>"
            }
        })
        .to_string(),
    )
}

async fn handle_request(
    req: &str,
    dispatcher: &Arc<Mutex<RpcDispatcher<NodeBackend>>>,
    config: &RpcHttpConfig,
) -> String {
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("OPTIONS", _) => cors_preflight(),
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
            if let Some(expected) = config.token.as_deref() {
                if method_requires_token(&parsed.method) {
                    match extract_bearer_token(req) {
                        Some(provided) if token_matches(expected, provided) => {}
                        _ => return unauthorized_response(),
                    }
                }
            }
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

fn cors_headers() -> &'static str {
    "Access-Control-Allow-Origin: *\r\n\
     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
     Access-Control-Allow-Headers: content-type, authorization\r\n\
     Access-Control-Max-Age: 86400\r\n"
}

fn cors_preflight() -> String {
    format!(
        "HTTP/1.1 204 No Content\r\n{cors}Connection: close\r\n\r\n",
        cors = cors_headers()
    )
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n{cors}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
        cors = cors_headers()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_bind("127.0.0.1:8545"));
        assert!(is_loopback_bind("localhost:8545"));
        assert!(is_loopback_bind("[::1]:8545"));
        assert!(!is_loopback_bind("0.0.0.0:8545"));
        assert!(!is_loopback_bind("192.168.1.10:8545"));
    }

    #[test]
    fn public_reads_skip_token() {
        assert!(!method_requires_token("agora_getDagTips"));
        assert!(!method_requires_token("agora_getBlock"));
        assert!(!method_requires_token("agora_getTransaction"));
        assert!(!method_requires_token("agora_getMempool"));
        assert!(!method_requires_token("agora_getNodeInfo"));
        assert!(!method_requires_token("agora_estimateFee"));
        assert!(!method_requires_token("agora_getConstitution"));
        assert!(!method_requires_token("agora_listProposals"));
        assert!(!method_requires_token("agora_listOffices"));
        assert!(method_requires_token("agora_submitTransaction"));
        assert!(method_requires_token("agora_submitBlock"));
        assert!(method_requires_token("agora_getBlockTemplate"));
        assert!(method_requires_token("agora_fundAddress"));
        assert!(method_requires_token("agora_getBalance"));
        assert!(method_requires_token("agora_getUtxos"));
        assert!(method_requires_token("agora_castGovVote"));
        assert!(method_requires_token("agora_submitProposal"));
    }

    #[test]
    fn bearer_extraction_and_compare() {
        let req =
            "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret-token\r\n\r\n{}";
        assert_eq!(extract_bearer_token(req), Some("secret-token"));
        assert!(token_matches("secret-token", "secret-token"));
        assert!(!token_matches("secret-token", "Secret-token"));
        assert!(!token_matches("ab", "abc"));
    }

    #[test]
    fn rate_limiter_caps_per_minute() {
        let mut lim = RateLimiter::default();
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        assert!(lim.allow(ip, 2));
        assert!(lim.allow(ip, 2));
        assert!(!lim.allow(ip, 2));
        assert!(lim.allow(ip, 0)); // disabled
    }
}
