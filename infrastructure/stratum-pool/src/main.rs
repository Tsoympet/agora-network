//! TCP stratum listener (JSON lines).
//!
//! Env:
//! - `AGORA_STRATUM_BIND` (default `0.0.0.0:3333`)

use std::sync::Arc;

use agora_stratum_pool::{StratumPool, StratumRequest, StratumResponse};
use agora_types::{BlockHeader, Hash};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let bind = std::env::var("AGORA_STRATUM_BIND").unwrap_or_else(|_| "0.0.0.0:3333".into());
    let pool = Arc::new(Mutex::new(StratumPool::new()));

    // Seed a genesis-difficulty job so miners can connect immediately.
    {
        let mut guard = pool.lock().await;
        guard.create_job(
            BlockHeader {
                version: 1,
                parents: vec![Hash::ZERO],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            0,
        );
    }

    let listener = TcpListener::bind(&bind).await.expect("bind stratum");
    info!(%bind, "agora-stratum-pool listening (kHeavyHash path)");

    loop {
        let (socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "accept failed");
                continue;
            }
        };
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_miner(socket, pool).await {
                warn!(%addr, error = %err, "miner session ended");
            }
        });
    }
}

async fn handle_miner(
    socket: tokio::net::TcpStream,
    pool: Arc<Mutex<StratumPool>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = socket.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut worker = String::from("anonymous");

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let req: StratumRequest = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                let resp = StratumResponse::err(None, -32700, format!("parse error: {err}"));
                writer
                    .write_all(format!("{}\n", serde_json::to_string(&resp)?).as_bytes())
                    .await?;
                continue;
            }
        };

        let resp = {
            let mut pool = pool.lock().await;
            match req.method.as_str() {
                "mining.subscribe" => StratumResponse::ok(
                    req.id.clone(),
                    serde_json::json!([["mining.notify", "agora"], "00"]),
                ),
                "mining.authorize" => {
                    let name = req
                        .params
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("worker");
                    worker = name.to_string();
                    pool.authorize(&worker);
                    StratumResponse::ok(req.id.clone(), serde_json::json!(true))
                }
                "mining.submit" => {
                    let params = req.params.as_array().cloned().unwrap_or_default();
                    let job_id = params
                        .get(1)
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let nonce = params
                        .get(2)
                        .and_then(|v| v.as_str())
                        .and_then(|s| u64::from_str_radix(s, 16).ok())
                        .or_else(|| params.get(2).and_then(|v| v.as_u64()))
                        .unwrap_or(0);
                    match pool.submit_share(&worker, job_id, nonce) {
                        Ok(_) => StratumResponse::ok(req.id.clone(), serde_json::json!(true)),
                        Err(err) => StratumResponse::err(req.id.clone(), 21, err.to_string()),
                    }
                }
                other => StratumResponse::err(
                    req.id.clone(),
                    -32601,
                    format!("method not found: {other}"),
                ),
            }
        };

        writer
            .write_all(format!("{}\n", serde_json::to_string(&resp)?).as_bytes())
            .await?;
    }
    Ok(())
}
