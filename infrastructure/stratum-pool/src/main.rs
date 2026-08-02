//! TCP stratum listener (JSON lines) backed by live `agora-node` templates.
//!
//! Env:
//! - `AGORA_STRATUM_BIND` (default `0.0.0.0:3333`)
//! - `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`)
//! - `AGORA_STRATUM_POLL_MS` (default `2000`)
//!
//! Node must run with `AGORA_POW_ALGO=kheavyhash` so submitted shares verify.
//!
//! New templates are broadcast to all connected miners via `mining.notify`.

use std::sync::Arc;

use agora_stratum_pool::node_rpc::{fetch_block_template, submit_block};
use agora_stratum_pool::{MiningJob, StratumPool, StratumRequest, StratumResponse};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tracing::{info, warn};

fn notify_payload(job: &MiningJob) -> serde_json::Value {
    json!({
        "id": null,
        "method": "mining.notify",
        "params": [
            job.job_id,
            job.block.header,
            job.difficulty_bits,
            job.block.transactions.len(),
        ],
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let bind = std::env::var("AGORA_STRATUM_BIND").unwrap_or_else(|_| "0.0.0.0:3333".into());
    let rpc_url =
        std::env::var("AGORA_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545/rpc".into());
    let poll_ms = std::env::var("AGORA_STRATUM_POLL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000u64);

    let pool = Arc::new(Mutex::new(StratumPool::new()));
    let (job_tx, _) = broadcast::channel::<MiningJob>(64);

    {
        let pool = pool.clone();
        let rpc_url = rpc_url.clone();
        let job_tx = job_tx.clone();
        tokio::spawn(async move {
            loop {
                match fetch_block_template(&rpc_url).await {
                    Ok(block) => {
                        let mut guard = pool.lock().await;
                        if let Some(job) = guard.upsert_template(block) {
                            info!(
                                job = %job.job_id,
                                bits = job.difficulty_bits,
                                txs = job.block.transactions.len(),
                                "installed live mining template"
                            );
                            let _ = job_tx.send(job);
                        }
                    }
                    Err(err) => warn!(error = %err, "template poll failed"),
                }
                tokio::time::sleep(std::time::Duration::from_millis(poll_ms)).await;
            }
        });
    }

    let listener = TcpListener::bind(&bind).await.expect("bind stratum");
    info!(%bind, %rpc_url, poll_ms, "agora-stratum-pool listening (kHeavyHash → node)");

    loop {
        let (socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "accept failed");
                continue;
            }
        };
        let pool = pool.clone();
        let rpc_url = rpc_url.clone();
        let job_rx = job_tx.subscribe();
        tokio::spawn(async move {
            if let Err(err) = handle_miner(socket, pool, rpc_url, job_rx).await {
                warn!(%addr, error = %err, "miner session ended");
            }
        });
    }
}

async fn handle_miner(
    socket: tokio::net::TcpStream,
    pool: Arc<Mutex<StratumPool>>,
    rpc_url: String,
    mut job_rx: broadcast::Receiver<MiningJob>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, writer) = socket.into_split();
    let writer = Arc::new(Mutex::new(writer));
    let mut lines = BufReader::new(reader).lines();
    let mut worker = String::from("anonymous");

    // Fan-out task: push mining.notify whenever the pool installs a new template.
    {
        let writer = writer.clone();
        tokio::spawn(async move {
            loop {
                match job_rx.recv().await {
                    Ok(job) => {
                        let line = match serde_json::to_string(&notify_payload(&job)) {
                            Ok(s) => format!("{s}\n"),
                            Err(_) => continue,
                        };
                        let mut w = writer.lock().await;
                        if w.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let req: StratumRequest = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                let resp = StratumResponse::err(None, -32700, format!("parse error: {err}"));
                let mut w = writer.lock().await;
                w.write_all(format!("{}\n", serde_json::to_string(&resp)?).as_bytes())
                    .await?;
                continue;
            }
        };

        let mut notify_job: Option<MiningJob> = None;
        let mut solved: Option<agora_types::Block> = None;

        let resp = {
            let mut pool = pool.lock().await;
            match req.method.as_str() {
                "mining.subscribe" => {
                    notify_job = pool.current_job().cloned();
                    StratumResponse::ok(
                        req.id.clone(),
                        json!([["mining.notify", "agora"], "00"]),
                    )
                }
                "mining.authorize" => {
                    let name = req
                        .params
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("worker");
                    worker = name.to_string();
                    pool.authorize(&worker);
                    notify_job = pool.current_job().cloned();
                    StratumResponse::ok(req.id.clone(), json!(true))
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
                        Ok(share) => {
                            solved = Some(share.block);
                            StratumResponse::ok(req.id.clone(), json!(true))
                        }
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

        {
            let mut w = writer.lock().await;
            w.write_all(format!("{}\n", serde_json::to_string(&resp)?).as_bytes())
                .await?;

            if let Some(job) = notify_job {
                w.write_all(format!("{}\n", serde_json::to_string(&notify_payload(&job))?).as_bytes())
                    .await?;
            }
        }

        if let Some(block) = solved {
            match submit_block(&rpc_url, &block).await {
                Ok(id) => info!(
                    %worker,
                    block = %id.to_hex(),
                    "submitted network share to agora-node"
                ),
                Err(err) => warn!(%worker, error = %err, "agora_submitBlock failed"),
            }
        }
    }
    Ok(())
}
