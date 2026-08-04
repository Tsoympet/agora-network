//! `agora-layers` — operator JSON-RPC for the L2/L3/L4 stack.
//!
//! Bind with `AGORA_LAYERS_BIND` (default `127.0.0.1:8555`).
//! Layer genesis:
//! - `AGORA_OVL_GENESIS_FILE` — Ovolos L2 (default: embedded testnet)
//! - `AGORA_DRC_GENESIS_FILE` — Drachma L3 (default: embedded testnet)

use std::sync::Arc;

use agora_bridge_sdk::DrachmaGenesis;
use agora_intent_engine::Intent;
use agora_layers_runtime::{LayersRuntime, LayersRuntimeConfig};
use agora_ovolos_rollup::{Batch, BatchCommitment, EvmTx, FraudProof, OvolosGenesis};
use agora_types::{Address, Amount, Hash};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let bind = std::env::var("AGORA_LAYERS_BIND").unwrap_or_else(|_| "127.0.0.1:8555".into());
    let challenge_window_ms = std::env::var("AGORA_LAYERS_CHALLENGE_MS")
        .ok()
        .and_then(|s| s.parse().ok());

    let ovolos_genesis = match std::env::var("AGORA_OVL_GENESIS_FILE") {
        Ok(path) => OvolosGenesis::from_path(&path).unwrap_or_else(|e| {
            panic!("failed to load AGORA_OVL_GENESIS_FILE ({path}): {e}");
        }),
        Err(_) => OvolosGenesis::testnet(),
    };
    let drachma_genesis = match std::env::var("AGORA_DRC_GENESIS_FILE") {
        Ok(path) => DrachmaGenesis::from_path(&path).unwrap_or_else(|e| {
            panic!("failed to load AGORA_DRC_GENESIS_FILE ({path}): {e}");
        }),
        Err(_) => DrachmaGenesis::testnet(),
    };

    info!(
        ovl_chain = %ovolos_genesis.chain_id,
        ovl_genesis = %ovolos_genesis.genesis_hash,
        drc_chain = %drachma_genesis.chain_id,
        drc_genesis = %drachma_genesis.genesis_hash,
        "loading layer genesis"
    );

    let runtime = LayersRuntime::new(LayersRuntimeConfig {
        challenge_window_ms,
        gas_payer: None,
        hub_id: None,
        ovolos_genesis,
        drachma_genesis,
    })
    .expect("layers runtime boot");
    let state = Arc::new(Mutex::new(runtime));
    serve(&bind, state).await;
}

async fn serve(bind: &str, state: Arc<Mutex<LayersRuntime>>) {
    let listener = TcpListener::bind(bind).await.expect("bind agora-layers");
    info!(%bind, "agora-layers listening (L2/L3/L4 JSON-RPC)");
    loop {
        let (mut socket, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "accept failed");
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let response = handle_http(&req, &state).await;
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(%peer, "served");
        });
    }
}

async fn handle_http(req: &str, state: &Arc<Mutex<LayersRuntime>>) -> String {
    let first = req.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    match (method, path) {
        ("GET", "/health") => http_json(200, json!({"ok": true, "service": "agora-layers"})),
        ("POST", "/rpc") | ("POST", "/") => {
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            let rpc: RpcReq = match serde_json::from_str(body) {
                Ok(v) => v,
                Err(err) => {
                    return http_json(
                        400,
                        json!({"jsonrpc":"2.0","error":{"code":-32700,"message":err.to_string()}}),
                    );
                }
            };
            let result = dispatch(
                rpc.method.as_str(),
                rpc.params.unwrap_or(Value::Null),
                state,
            )
            .await;
            match result {
                Ok(value) => http_json(200, json!({"jsonrpc":"2.0","id": rpc.id, "result": value})),
                Err(err) => http_json(
                    200,
                    json!({"jsonrpc":"2.0","id": rpc.id, "error":{"code":-32000,"message":err}}),
                ),
            }
        }
        _ => http_json(404, json!({"error":"not found"})),
    }
}

#[derive(Debug, Deserialize)]
struct RpcReq {
    #[serde(default)]
    id: Value,
    method: String,
    params: Option<Value>,
}

async fn dispatch(
    method: &str,
    params: Value,
    state: &Arc<Mutex<LayersRuntime>>,
) -> Result<Value, String> {
    let mut rt = state.lock().await;
    match method {
        "agora_layers_getInfo" => Ok(serde_json::to_value(rt.info()).map_err(|e| e.to_string())?),
        "agora_layers_mintOvl" => {
            let p: AddrAmount = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let addr = parse_addr(&p.address)?;
            rt.mint_ovl(addr, Amount::from_base_units(p.amount))
                .map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_submitBatch" => {
            let p: BatchParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let batch = batch_from_params(&p)?;
            let id = rt.submit_batch(batch).map_err(|e| e.to_string())?;
            Ok(json!({"batch_id": id.to_hex()}))
        }
        "agora_layers_submitBatchAs" => {
            let p: BatchAsParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let batch = batch_from_params(&p.batch)?;
            let id = rt
                .submit_batch_as(parse_addr(&p.sequencer)?, batch)
                .map_err(|e| e.to_string())?;
            Ok(json!({"batch_id": id.to_hex()}))
        }
        "agora_layers_recordDa" => {
            let p: CommitmentParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let c = BatchCommitment {
                batch_id: parse_hash(&p.batch_id)?,
                sequence: p.sequence,
                prev_state_root: parse_hash(&p.prev_state_root)?,
                post_state_root: parse_hash(&p.post_state_root)?,
                tx_merkle_root: parse_hash(&p.tx_merkle_root)?,
                tx_count: p.tx_count,
                posted_at_ms: p.posted_at_ms,
            };
            rt.record_da(c).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_challenge" => {
            let p: ChallengeParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let proof = FraudProof {
                batch_id: parse_hash(&p.batch_id)?,
                claimed_post_state_root: parse_hash(&p.claimed_post_state_root)?,
                computed_post_state_root: parse_hash(&p.computed_post_state_root)?,
                diverging_tx_index: p.diverging_tx_index,
            };
            rt.challenge(proof).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_finalizeDue" => {
            let now_ms = params
                .get("now_ms")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "now_ms required".to_string())?;
            let ids = rt.finalize_due(now_ms).map_err(|e| e.to_string())?;
            Ok(json!({"finalized": ids.iter().map(|h| h.to_hex()).collect::<Vec<_>>()}))
        }
        "agora_layers_finalizeDueAs" => {
            let p: SequencerNowParams =
                serde_json::from_value(params).map_err(|e| e.to_string())?;
            let ids = rt
                .finalize_due_as(parse_addr(&p.sequencer)?, p.now_ms)
                .map_err(|e| e.to_string())?;
            Ok(json!({"finalized": ids.iter().map(|h| h.to_hex()).collect::<Vec<_>>()}))
        }
        "agora_layers_bondSequencer" => {
            let p: AddrAmount = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let bonded = rt
                .bond_sequencer(parse_addr(&p.address)?, Amount::from_base_units(p.amount))
                .map_err(|e| e.to_string())?;
            Ok(json!({"bonded": bonded}))
        }
        "agora_layers_bondAttestor" => {
            let p: AddrAmount = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let bonded = rt
                .bond_attestor(parse_addr(&p.address)?, Amount::from_base_units(p.amount))
                .map_err(|e| e.to_string())?;
            Ok(json!({"bonded": bonded}))
        }
        "agora_layers_attestMessage" => {
            let p: AttestParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let quorum = rt
                .attest_message(parse_addr(&p.attestor)?, parse_hash(&p.message_id)?)
                .map_err(|e| e.to_string())?;
            Ok(json!({"quorum_reached": quorum}))
        }
        "agora_layers_creditDrc" => {
            let p: CreditParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let addr = parse_addr(&p.address)?;
            rt.credit_drc(&p.hub, addr, Amount::from_base_units(p.amount))
                .map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_lockAndMint" => {
            let p: BridgeParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let id = rt
                .lock_and_mint_tagged(
                    &p.source,
                    &p.dest,
                    parse_addr(&p.sender)?,
                    parse_addr(&p.recipient)?,
                    Amount::from_base_units(p.amount),
                    p.nonce,
                    p.destination_tag.unwrap_or(0),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "message_id": id.to_hex(),
                "destination_tag": p.destination_tag.unwrap_or(0),
            }))
        }
        "agora_layers_claimMint" => {
            let id = params
                .get("message_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "message_id required".to_string())?;
            rt.claim_mint(parse_hash(id)?).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_submitIntent" => {
            let p: IntentParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let recipient = match &p.recipient {
                Some(r) => parse_addr(r)?,
                None => Address::ZERO,
            };
            let intent = Intent {
                id_salt: p.id_salt,
                user: parse_addr(&p.user)?,
                give_asset_district: p.give_asset_district,
                give_amount: Amount::from_base_units(p.give_amount),
                want_asset_district: p.want_asset_district,
                min_receive: Amount::from_base_units(p.min_receive),
                deadline_ms: p.deadline_ms,
                solver_hint: p.solver_hint.unwrap_or_default(),
                recipient,
                destination_tag: p.destination_tag.unwrap_or(0),
            };
            let id = rt
                .submit_intent(intent, p.now_ms.unwrap_or(0))
                .map_err(|e| e.to_string())?;
            Ok(json!({"intent_id": id.to_hex()}))
        }
        "agora_layers_settleIntent" => {
            let id = params
                .get("intent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "intent_id required".to_string())?;
            let now_ms = params.get("now_ms").and_then(|v| v.as_u64()).unwrap_or(0);
            let intent_id = parse_hash(id)?;
            let sol = rt
                .settle_intent(intent_id, now_ms)
                .map_err(|e| e.to_string())?;
            let status = rt.intent_status(&intent_id);
            Ok(json!({
                "receive_amount": sol.receive_amount.as_base_units(),
                "route": sol.route,
                "strategy": sol.strategy,
                "status": format!("{status:?}"),
            }))
        }
        "agora_layers_finalizeIntent" => {
            let id = params
                .get("intent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "intent_id required".to_string())?;
            rt.finalize_intent(parse_hash(id)?)
                .map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_messageStatus" => {
            let id = params
                .get("message_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "message_id required".to_string())?;
            let status = rt.message_status(&parse_hash(id)?);
            Ok(json!({"status": format!("{status:?}")}))
        }
        "agora_layers_registerDestinationTag" => {
            let p: TagParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            rt.register_destination_tag(&p.district, parse_addr(&p.owner)?, p.tag)
                .map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "agora_layers_paymentsForTag" => {
            let p: TagQuery = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let ids = rt.payments_for_tag(&p.district, p.tag);
            Ok(json!({
                "payments": ids.iter().map(|h| h.to_hex()).collect::<Vec<_>>(),
                "owner": rt.destination_tag_owner(&p.district, p.tag).map(|a| a.to_hex()),
            }))
        }
        "agora_layers_unbondAttestor" => {
            let p: AddrAmount = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let remaining = rt
                .unbond_attestor(parse_addr(&p.address)?, Amount::from_base_units(p.amount))
                .map_err(|e| e.to_string())?;
            Ok(json!({"remaining_bond": remaining}))
        }
        "agora_layers_unbondSequencer" => {
            let p: AddrAmount = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let remaining = rt
                .unbond_sequencer(parse_addr(&p.address)?, Amount::from_base_units(p.amount))
                .map_err(|e| e.to_string())?;
            Ok(json!({"remaining_bond": remaining}))
        }
        "agora_layers_getOvlBalance" => {
            let addr = params
                .get("address")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "address required".to_string())?;
            Ok(json!({
                "balance": rt.ovl_balance(parse_addr(addr)?).as_base_units()
            }))
        }
        "agora_layers_getDrcBalance" => {
            let district = params
                .get("district")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "district required".to_string())?;
            let addr = params
                .get("address")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "address required".to_string())?;
            Ok(json!({
                "balance": rt.drc_balance(district, parse_addr(addr)?).as_base_units()
            }))
        }
        "agora_layers_mineOvlBlock" => {
            let p: MineOvlParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let batch = Batch {
                sequence: p.sequence,
                prev_state_root: parse_hash(&p.prev_state_root)?,
                post_state_root: parse_hash(&p.post_state_root)?,
                transactions: {
                    let mut txs = Vec::with_capacity(p.transactions.len());
                    for t in p.transactions {
                        let raw =
                            hex::decode(t.trim_start_matches("0x")).map_err(|e| e.to_string())?;
                        txs.push(EvmTx(raw));
                    }
                    txs
                },
                posted_at_ms: p.posted_at_ms,
            };
            let block = rt
                .mine_ovl_block(
                    batch,
                    parse_addr(&p.miner)?,
                    p.timestamp_ms,
                    p.max_nonces.unwrap_or(1_000_000),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "block_id": block.id().to_hex(),
                "height": block.header.height,
                "nonce": block.header.nonce,
                "reward": block.header.reward,
                "batch_id": block.header.batch_id.to_hex(),
            }))
        }
        "agora_layers_mineDrcBlock" => {
            let p: MineDrcParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let mut ids = Vec::with_capacity(p.message_ids.len());
            for id in p.message_ids {
                ids.push(parse_hash(&id)?);
            }
            let block = rt
                .mine_drc_block(
                    &p.district_id,
                    ids,
                    parse_addr(&p.miner)?,
                    p.timestamp_ms,
                    p.max_nonces.unwrap_or(1_000_000),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "block_id": block.id().to_hex(),
                "district_id": block.header.district_id,
                "height": block.header.height,
                "nonce": block.header.nonce,
                "reward": block.header.reward,
            }))
        }
        "agora_layers_payDrc" => {
            let p: PayParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let id = rt
                .pay_drc(
                    &p.district,
                    parse_addr(&p.sender)?,
                    parse_addr(&p.recipient)?,
                    Amount::from_base_units(p.amount),
                    p.nonce,
                    p.destination_tag.unwrap_or(0),
                )
                .map_err(|e| e.to_string())?;
            Ok(
                json!({"payment_id": id.to_hex(), "destination_tag": p.destination_tag.unwrap_or(0)}),
            )
        }
        "agora_layers_pathPayDrc" => {
            let p: PathPayParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let (unlock_id, mint_id) = rt
                .path_pay_drc_deliver(
                    &p.hub,
                    &p.source,
                    &p.dest,
                    parse_addr(&p.sender)?,
                    parse_addr(&p.recipient)?,
                    Amount::from_base_units(p.amount),
                    p.nonce,
                    p.destination_tag.unwrap_or(0),
                    Amount::from_base_units(p.deliver_min.unwrap_or(0)),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "unlock_id": unlock_id.to_hex(),
                "mint_id": mint_id.to_hex(),
                "destination_tag": p.destination_tag.unwrap_or(0),
                "deliver_min": p.deliver_min.unwrap_or(0),
            }))
        }
        "agora_layers_burnAndUnlock" => {
            let p: BridgeParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let id = rt
                .burn_and_unlock(
                    &p.source,
                    &p.dest,
                    parse_addr(&p.sender)?,
                    parse_addr(&p.recipient)?,
                    Amount::from_base_units(p.amount),
                    p.nonce,
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({"message_id": id.to_hex()}))
        }
        // Ethereum-class L2 surface (OVL = ETH role on Ovolos).
        "eth_chainId" => Ok(json!(format!("0x{:x}", rt.eth_chain_id()))),
        "eth_blockNumber" => Ok(json!(format!("0x{:x}", rt.eth_block_number()))),
        "eth_getBalance" => {
            let addr = params
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .ok_or_else(|| "eth_getBalance(address, block) required".to_string())?;
            let bal = rt.eth_get_balance(parse_addr(addr)?);
            Ok(json!(format!("0x{:x}", bal)))
        }
        "eth_getTransactionCount" => {
            let addr = params
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .ok_or_else(|| "eth_getTransactionCount(address, block) required".to_string())?;
            let n = rt.eth_get_transaction_count(parse_addr(addr)?);
            Ok(json!(format!("0x{:x}", n)))
        }
        "eth_getCode" => {
            let addr = params
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .ok_or_else(|| "eth_getCode(address, block) required".to_string())?;
            let code = rt.eth_get_code(parse_addr(addr)?);
            Ok(json!(format!("0x{}", hex::encode(code))))
        }
        "eth_getStorageAt" => {
            let arr = params
                .as_array()
                .ok_or_else(|| "eth_getStorageAt(address, slot, block) required".to_string())?;
            let addr = arr
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| "address required".to_string())?;
            let slot_hex = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| "slot required".to_string())?;
            let slot_bytes =
                hex::decode(slot_hex.trim_start_matches("0x")).map_err(|e| e.to_string())?;
            if slot_bytes.len() > 32 {
                return Err("slot too long".into());
            }
            let mut slot = [0u8; 32];
            slot[32 - slot_bytes.len()..].copy_from_slice(&slot_bytes);
            let value = rt.eth_get_storage_at(parse_addr(addr)?, slot);
            Ok(json!(format!("0x{}", hex::encode(value))))
        }
        "eth_call" => {
            let obj = params
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .ok_or_else(|| "eth_call(tx, block) required".to_string())?;
            let to = obj
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "to required".to_string())?;
            let data = obj.get("data").and_then(|v| v.as_str()).unwrap_or("0x");
            let value = obj
                .get("value")
                .and_then(|v| v.as_str())
                .map(parse_hex_u128)
                .transpose()?
                .unwrap_or(0);
            let data_bytes =
                hex::decode(data.trim_start_matches("0x")).map_err(|e| e.to_string())?;
            let out = rt
                .eth_call(parse_addr(to)?, &data_bytes, value)
                .map_err(|e| e.to_string())?;
            Ok(json!(format!("0x{}", hex::encode(out))))
        }
        "eth_sendRawTransaction" => {
            let raw_hex = params
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .ok_or_else(|| "eth_sendRawTransaction(raw) required".to_string())?;
            let raw = hex::decode(raw_hex.trim_start_matches("0x")).map_err(|e| e.to_string())?;
            let id = rt
                .eth_send_raw_transaction(EvmTx(raw))
                .map_err(|e| e.to_string())?;
            Ok(json!(format!("0x{}", id.to_hex())))
        }
        "agora_layers_drainL2Mempool" => {
            let txs = rt.drain_l2_mempool();
            Ok(json!({
                "transactions": txs.iter().map(|t| format!("0x{}", hex::encode(&t.0))).collect::<Vec<_>>(),
                "count": txs.len(),
            }))
        }
        _ => Err(format!("unknown method {method}")),
    }
}

fn parse_hex_u128(s: &str) -> Result<u128, String> {
    let s = s.trim_start_matches("0x");
    if s.is_empty() {
        return Ok(0);
    }
    u128::from_str_radix(s, 16).map_err(|e| e.to_string())
}

fn batch_from_params(p: &BatchParams) -> Result<Batch, String> {
    let mut txs = Vec::with_capacity(p.transactions.len());
    for t in &p.transactions {
        let raw = hex::decode(t.trim_start_matches("0x")).map_err(|e| e.to_string())?;
        txs.push(EvmTx(raw));
    }
    Ok(Batch {
        sequence: p.sequence,
        prev_state_root: parse_hash(&p.prev_state_root)?,
        post_state_root: parse_hash(&p.post_state_root)?,
        transactions: txs,
        posted_at_ms: p.posted_at_ms,
    })
}

fn parse_addr(s: &str) -> Result<Address, String> {
    Address::parse(s).ok_or_else(|| format!("invalid address {s}"))
}

fn parse_hash(s: &str) -> Result<Hash, String> {
    Hash::from_hex(s).ok_or_else(|| format!("invalid hash {s}"))
}

fn http_json(status: u16, body: Value) -> String {
    let body = body.to_string();
    format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[derive(Debug, Deserialize)]
struct AddrAmount {
    address: String,
    amount: u64,
}

#[derive(Debug, Deserialize)]
struct BatchParams {
    sequence: u64,
    prev_state_root: String,
    post_state_root: String,
    transactions: Vec<String>,
    posted_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct BatchAsParams {
    sequencer: String,
    #[serde(flatten)]
    batch: BatchParams,
}

#[derive(Debug, Deserialize)]
struct SequencerNowParams {
    sequencer: String,
    now_ms: u64,
}

#[derive(Debug, Deserialize)]
struct AttestParams {
    attestor: String,
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct CommitmentParams {
    batch_id: String,
    sequence: u64,
    prev_state_root: String,
    post_state_root: String,
    tx_merkle_root: String,
    tx_count: u32,
    posted_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct ChallengeParams {
    batch_id: String,
    claimed_post_state_root: String,
    computed_post_state_root: String,
    diverging_tx_index: u32,
}

#[derive(Debug, Deserialize)]
struct CreditParams {
    hub: String,
    address: String,
    amount: u64,
}

#[derive(Debug, Deserialize)]
struct BridgeParams {
    source: String,
    dest: String,
    sender: String,
    recipient: String,
    amount: u64,
    nonce: u64,
    destination_tag: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct IntentParams {
    id_salt: u64,
    user: String,
    give_asset_district: String,
    give_amount: u64,
    want_asset_district: String,
    min_receive: u64,
    deadline_ms: u64,
    solver_hint: Option<String>,
    now_ms: Option<u64>,
    recipient: Option<String>,
    destination_tag: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TagParams {
    district: String,
    owner: String,
    tag: u32,
}

#[derive(Debug, Deserialize)]
struct TagQuery {
    district: String,
    tag: u32,
}

#[derive(Debug, Deserialize)]
struct MineOvlParams {
    sequence: u64,
    prev_state_root: String,
    post_state_root: String,
    transactions: Vec<String>,
    posted_at_ms: u64,
    miner: String,
    timestamp_ms: u64,
    max_nonces: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MineDrcParams {
    district_id: String,
    message_ids: Vec<String>,
    miner: String,
    timestamp_ms: u64,
    max_nonces: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PayParams {
    district: String,
    sender: String,
    recipient: String,
    amount: u64,
    nonce: u64,
    destination_tag: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PathPayParams {
    hub: String,
    source: String,
    dest: String,
    sender: String,
    recipient: String,
    amount: u64,
    nonce: u64,
    destination_tag: Option<u32>,
    deliver_min: Option<u64>,
}
