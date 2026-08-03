//! HTTP DNS-seeder client — pull / register bootstrap multiaddrs.
//!
//! Talks to `agora-dns-seeder` (`GET /peers`, `POST /peers`) over plain HTTP/1.1
//! with tokio TCP (no extra HTTP client crate).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

use crate::P2pError;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpUrl {
    host_port: String,
    path: String,
}

fn parse_http_url(url: &str) -> Result<HttpUrl, P2pError> {
    let trimmed = url.trim();
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    if rest.is_empty() {
        return Err(P2pError::Seeder("empty seeder url".into()));
    }
    let (host_port, path) = match rest.find('/') {
        Some(i) => {
            let path = &rest[i..];
            (
                rest[..i].to_string(),
                if path.is_empty() {
                    "/peers".into()
                } else {
                    path.to_string()
                },
            )
        }
        None => (rest.to_string(), "/peers".into()),
    };
    if host_port.is_empty() {
        return Err(P2pError::Seeder("missing host in seeder url".into()));
    }
    Ok(HttpUrl { host_port, path })
}

async fn http_exchange(host_port: &str, request: &str) -> Result<String, P2pError> {
    let mut stream = TcpStream::connect(host_port)
        .await
        .map_err(|e| P2pError::Seeder(format!("connect {host_port}: {e}")))?;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| P2pError::Seeder(format!("write: {e}")))?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| P2pError::Seeder(format!("read: {e}")))?;
    let text = String::from_utf8_lossy(&buf);
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| P2pError::Seeder("missing HTTP body".into()))?
        .trim()
        .to_string();
    let status_line = text.lines().next().unwrap_or("");
    if !status_line.contains("200") {
        return Err(P2pError::Seeder(format!(
            "unexpected status: {status_line}; body={body}"
        )));
    }
    Ok(body)
}

/// `GET` peer multiaddrs from a seeder URL (`http://host:port/peers`).
pub async fn fetch_seeder_peers(url: &str) -> Result<Vec<String>, P2pError> {
    let parsed = parse_http_url(url)?;
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        parsed.path, parsed.host_port
    );
    let body = http_exchange(&parsed.host_port, &req).await?;
    let peers: Vec<String> = serde_json::from_str(&body)
        .map_err(|e| P2pError::Seeder(format!("peers json: {e}; body={body}")))?;
    let peers: Vec<String> = peers
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    info!(url, count = peers.len(), "fetched dns seeder peers");
    Ok(peers)
}

/// `POST` a dialable multiaddr to the seeder phonebook.
///
/// When `AGORA_SEEDER_TOKEN` is set, sends `Authorization: Bearer …` (required
/// by `agora-dns-seeder` for authenticated public registration).
pub async fn register_with_seeder(url: &str, multiaddr: &str) -> Result<(), P2pError> {
    let parsed = parse_http_url(url)?;
    let path = if parsed.path.ends_with("/peers") {
        parsed.path
    } else {
        "/peers".into()
    };
    let body = multiaddr.trim();
    let auth = std::env::var("AGORA_SEEDER_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n{auth}Connection: close\r\n\r\n{body}",
        parsed.host_port,
        body.len(),
    );
    let _ = http_exchange(&parsed.host_port, &req).await?;
    debug!(url, multiaddr, "registered with dns seeder");
    Ok(())
}

/// Normalize a seeder base URL (host or `http://host:port`) to `http://…/peers`.
pub fn normalize_seeder_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with("/peers") {
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return trimmed.to_string();
        }
        return format!("http://{trimmed}");
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        format!("{trimmed}/peers")
    } else {
        format!("http://{trimmed}/peers")
    }
}

/// Merge bootstrap + seeder peers, capping at `max_peers`, preserving order / uniqueness.
pub fn merge_bootstrap_peers(
    bootstrap: &[String],
    seeder: &[String],
    max_peers: u32,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for peer in bootstrap.iter().chain(seeder.iter()) {
        let p = peer.trim();
        if p.is_empty() || !seen.insert(p.to_string()) {
            continue;
        }
        out.push(p.to_string());
        if out.len() >= max_peers as usize {
            break;
        }
    }
    out
}

/// Best-effort fetch; logs and returns empty on failure so node boot continues.
pub async fn fetch_seeder_peers_best_effort(url: &str) -> Vec<String> {
    match fetch_seeder_peers(url).await {
        Ok(peers) => peers,
        Err(err) => {
            warn!(url, error = %err, "dns seeder fetch failed");
            Vec::new()
        }
    }
}

/// Tracks seeder registration + periodic peer refresh / re-dial.
#[derive(Debug, Clone)]
pub struct SeederBook {
    url: String,
    max_peers: u32,
    bootstrap: Vec<String>,
    /// Multiaddrs we have already attempted to dial.
    dialed: std::collections::HashSet<String>,
    /// Last known dialable listen multiaddr (with `/p2p/<id>`).
    dialable: Option<String>,
    refresh_interval: std::time::Duration,
}

impl SeederBook {
    pub fn new(
        url: impl Into<String>,
        bootstrap: Vec<String>,
        max_peers: u32,
        refresh_interval: std::time::Duration,
    ) -> Self {
        let mut dialed = std::collections::HashSet::new();
        for peer in &bootstrap {
            dialed.insert(peer.clone());
        }
        Self {
            url: normalize_seeder_url(&url.into()),
            max_peers: max_peers.max(1),
            bootstrap,
            dialed,
            dialable: None,
            refresh_interval,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn refresh_interval(&self) -> std::time::Duration {
        self.refresh_interval
    }

    pub fn dialable(&self) -> Option<&str> {
        self.dialable.as_deref()
    }

    pub fn note_dialed(&mut self, peers: &[String]) {
        for peer in peers {
            self.dialed.insert(peer.clone());
        }
    }

    pub fn has_dialed(&self, peer: &str) -> bool {
        self.dialed.contains(peer)
    }

    pub fn dialed_count(&self) -> usize {
        self.dialed.len()
    }

    /// Update the dialable listen address used for seeder registration.
    pub fn set_dialable(&mut self, multiaddr: impl Into<String>) {
        self.dialable = Some(multiaddr.into());
    }

    /// Register (or re-register) the current dialable address with the seeder.
    pub async fn register(&self) -> Result<(), P2pError> {
        let Some(addr) = &self.dialable else {
            return Err(P2pError::Seeder("no dialable address yet".into()));
        };
        register_with_seeder(&self.url, addr).await
    }

    /// Fetch seeder peers, dial any not yet attempted (up to `max_peers`), re-register.
    ///
    /// Returns newly dialed multiaddrs.
    pub async fn refresh_and_dial(
        &mut self,
        dial: impl Fn(&str) -> Result<(), P2pError>,
    ) -> Vec<String> {
        let seeder = fetch_seeder_peers_best_effort(&self.url).await;
        let merged = merge_bootstrap_peers(&self.bootstrap, &seeder, self.max_peers);
        let mut newly = Vec::new();
        for peer in merged {
            if self.dialed.contains(&peer) {
                continue;
            }
            match dial(&peer) {
                Ok(()) => {
                    info!(peer, "seeder refresh dialed peer");
                    self.dialed.insert(peer.clone());
                    newly.push(peer);
                }
                Err(err) => {
                    warn!(peer, error = %err, "seeder refresh dial failed");
                    // Still mark dialed so we do not spam the same bad multiaddr every tick.
                    self.dialed.insert(peer);
                }
            }
        }
        if let Err(err) = self.register().await {
            warn!(error = %err, "seeder refresh re-register failed");
        } else if self.dialable.is_some() {
            debug!(url = %self.url, "seeder refresh re-registered dialable addr");
        }
        newly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn parses_and_normalizes_urls() {
        assert_eq!(
            normalize_seeder_url("127.0.0.1:18080"),
            "http://127.0.0.1:18080/peers"
        );
        assert_eq!(
            normalize_seeder_url("http://127.0.0.1:18080"),
            "http://127.0.0.1:18080/peers"
        );
        assert_eq!(
            normalize_seeder_url("http://127.0.0.1:18080/peers"),
            "http://127.0.0.1:18080/peers"
        );
        let u = parse_http_url("http://127.0.0.1:18080/peers").unwrap();
        assert_eq!(u.host_port, "127.0.0.1:18080");
        assert_eq!(u.path, "/peers");
    }

    #[test]
    fn merges_unique_capped_peers() {
        let merged = merge_bootstrap_peers(
            &["/ip4/1.1.1.1/tcp/1".into(), "/ip4/2.2.2.2/tcp/2".into()],
            &["/ip4/2.2.2.2/tcp/2".into(), "/ip4/3.3.3.3/tcp/3".into()],
            2,
        );
        assert_eq!(
            merged,
            vec![
                "/ip4/1.1.1.1/tcp/1".to_string(),
                "/ip4/2.2.2.2/tcp/2".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn seeder_book_tracks_dialable_and_skips_known() {
        let mut book = SeederBook::new(
            "http://127.0.0.1:9/peers",
            vec!["/ip4/1.1.1.1/tcp/1".into()],
            8,
            std::time::Duration::from_secs(60),
        );
        book.note_dialed(&["/ip4/2.2.2.2/tcp/2".into()]);
        book.set_dialable("/ip4/127.0.0.1/tcp/16111/p2p/12D3KooWLocal");
        assert_eq!(
            book.dialable(),
            Some("/ip4/127.0.0.1/tcp/16111/p2p/12D3KooWLocal")
        );
        assert!(book.has_dialed("/ip4/1.1.1.1/tcp/1"));
        assert!(book.has_dialed("/ip4/2.2.2.2/tcp/2"));
        assert_eq!(book.dialed_count(), 2);
    }

    #[tokio::test]
    async fn seeder_book_refresh_dials_and_registers() {
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // GET peers, then POST register (may repeat).
            for _ in 0..4 {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let body = if req.starts_with("GET") {
                    r#"["/ip4/8.8.8.8/tcp/16111/p2p/12D3KooWFreshPeer"]"#
                } else {
                    "registered"
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let url = format!("http://{addr}/peers");
        let mut book = SeederBook::new(&url, Vec::new(), 8, std::time::Duration::from_secs(30));
        book.set_dialable("/ip4/127.0.0.1/tcp/16111/p2p/12D3KooWLocal");
        let dialed = Arc::new(Mutex::new(Vec::new()));
        let dialed_c = dialed.clone();
        let newly = book
            .refresh_and_dial(|peer| {
                dialed_c.lock().unwrap().push(peer.to_string());
                Ok(())
            })
            .await;
        assert_eq!(newly.len(), 1);
        assert!(newly[0].contains("8.8.8.8"));
        // Second refresh should not re-dial the same peer.
        let newly2 = book
            .refresh_and_dial(|peer| {
                dialed.lock().unwrap().push(peer.to_string());
                Ok(())
            })
            .await;
        assert!(newly2.is_empty());
    }

    #[tokio::test]
    async fn fetch_and_register_against_mock_seeder() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // Serve GET then POST.
            for expected_method in ["GET", "POST"] {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                assert!(req.starts_with(expected_method));
                let body = if expected_method == "GET" {
                    r#"["/ip4/9.9.9.9/tcp/16111/p2p/12D3KooWTestPeer000000000000000000000000"]"#
                } else {
                    "registered"
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                sock.write_all(resp.as_bytes()).await.unwrap();
            }
        });

        let url = format!("http://{addr}/peers");
        let peers = fetch_seeder_peers(&url).await.unwrap();
        assert_eq!(peers.len(), 1);
        assert!(peers[0].contains("9.9.9.9"));

        register_with_seeder(&url, "/ip4/127.0.0.1/tcp/16111/p2p/12D3KooWLocal")
            .await
            .unwrap();
    }
}
