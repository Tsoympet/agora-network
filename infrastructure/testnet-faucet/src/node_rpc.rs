//! HTTP JSON-RPC client for live `agora-node` funding.

use agora_rpc::{RpcRequest, RpcResponse};
use agora_types::{Address, Amount};
use serde_json::json;

pub async fn fund_address(
    rpc_url: &str,
    address: Address,
    amount: Amount,
) -> Result<Amount, String> {
    let resp = rpc_call(
        rpc_url,
        "agora_fundAddress",
        json!({
            "address": address.to_hex(),
            "amount": amount.as_base_units(),
        }),
    )
    .await?;
    let value = resp
        .result
        .ok_or_else(|| format!("fund error: {:?}", resp.error))?;
    let units = value
        .get("balance")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("missing balance in {value}"))?;
    Ok(Amount::from_base_units(units))
}

pub async fn get_balance(rpc_url: &str, address: &Address) -> Result<Amount, String> {
    let resp = rpc_call(
        rpc_url,
        "agora_getBalance",
        json!([address.to_hex()]),
    )
    .await?;
    let value = resp
        .result
        .ok_or_else(|| format!("balance error: {:?}", resp.error))?;
    let units = value
        .get("balance")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("missing balance in {value}"))?;
    Ok(Amount::from_base_units(units))
}

async fn rpc_call(url: &str, method: &str, params: serde_json::Value) -> Result<RpcResponse, String> {
    let body = serde_json::to_string(&RpcRequest {
        id: Some(json!(1)),
        method: method.into(),
        params,
    })
    .map_err(|e| e.to_string())?;

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
        rest.find('/')
            .map(|i| rest[i..].to_string())
            .unwrap_or_else(|| "/rpc".into())
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
