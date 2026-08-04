//! Non-mint faucet funding: signed spends from a treasury UTXO set.
//!
//! Replaces `agora_fundAddress` mints for public testnet. The treasury key is the
//! BIP-44 external(0) account from `AGORA_FAUCET_MNEMONIC` (testnet premine uses
//! the well-known `abandon … about` phrase).

use agora_crypto::{derive_bip44, seed_from_mnemonic, sign_transaction, Bip44Path};
use agora_types::{Address, Amount, Hash, OutPoint, Transaction, TxIn, TxOut};
use serde_json::json;

use crate::node_rpc;

/// Default relay fee charged on each treasury drip (base units).
pub const TREASURY_DRIP_FEE: u64 = 1;

#[derive(Debug, Clone)]
struct RpcUtxo {
    tx_id: Hash,
    index: u32,
    value: u64,
}

/// Build, sign, and submit a treasury → recipient transfer; return recipient balance.
pub async fn drip_from_treasury(
    rpc_url: &str,
    mnemonic: &str,
    recipient: Address,
    amount: Amount,
) -> Result<Amount, String> {
    let seed = seed_from_mnemonic(mnemonic, "").map_err(|e| e.to_string())?;
    let kp = derive_bip44(&seed, &Bip44Path::external(0)).map_err(|e| e.to_string())?;
    let change_kp = derive_bip44(&seed, &Bip44Path::new(0, 1, 0)).map_err(|e| e.to_string())?;
    let treasury = kp.address();

    let utxos = get_utxos(rpc_url, &treasury).await?;
    let need = amount
        .as_base_units()
        .checked_add(TREASURY_DRIP_FEE)
        .ok_or_else(|| "drip amount overflow".to_string())?;

    let mut selected = Vec::new();
    let mut total_in = 0u64;
    let mut sorted = utxos;
    sorted.sort_by(|a, b| b.value.cmp(&a.value));
    for u in sorted {
        total_in = total_in.saturating_add(u.value);
        selected.push(u);
        if total_in >= need {
            break;
        }
    }
    if total_in < need {
        return Err(format!(
            "treasury insufficient: have {total_in}, need {need}"
        ));
    }

    let change = total_in - need;
    let mut outputs = vec![TxOut {
        value: amount,
        address: recipient,
    }];
    if change > 0 {
        outputs.push(TxOut {
            value: Amount::from_base_units(change),
            address: change_kp.address(),
        });
    }

    let inputs: Vec<TxIn> = selected
        .iter()
        .map(|u| TxIn {
            previous_outpoint: OutPoint {
                tx_id: u.tx_id,
                index: u.index,
            },
        })
        .collect();

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let mut tx = Transaction::unsigned(1, inputs, outputs, nonce);
    sign_transaction(&mut tx, &kp).map_err(|e| e.to_string())?;

    submit_transaction(rpc_url, &tx).await?;
    node_rpc::get_balance(rpc_url, &recipient).await
}

async fn get_utxos(rpc_url: &str, address: &Address) -> Result<Vec<RpcUtxo>, String> {
    let resp = node_rpc::rpc_call(
        rpc_url,
        "agora_getUtxos",
        json!({ "address": address.to_hex() }),
    )
    .await?;
    let value = resp
        .result
        .ok_or_else(|| format!("getUtxos error: {:?}", resp.error))?;
    let list = value
        .get("utxos")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("missing utxos in {value}"))?;
    let mut out = Vec::with_capacity(list.len());
    for u in list {
        let tx_hex = u
            .get("tx_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "utxo missing tx_id".to_string())?;
        let tx_id = Hash::from_hex(tx_hex).ok_or_else(|| format!("bad tx_id {tx_hex}"))?;
        let index = u
            .get("index")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "utxo missing index".to_string())? as u32;
        let value = u
            .get("value")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "utxo missing value".to_string())?;
        out.push(RpcUtxo {
            tx_id,
            index,
            value,
        });
    }
    Ok(out)
}

async fn submit_transaction(rpc_url: &str, tx: &Transaction) -> Result<Hash, String> {
    let resp = node_rpc::rpc_call(
        rpc_url,
        "agora_submitTransaction",
        serde_json::to_value(tx).map_err(|e| e.to_string())?,
    )
    .await?;
    let value = resp
        .result
        .ok_or_else(|| format!("submit error: {:?}", resp.error))?;
    let id = value
        .get("tx_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing tx_id in {value}"))?;
    Hash::from_hex(id).ok_or_else(|| format!("bad tx_id {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treasury_address_matches_abandon_external0() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let seed = seed_from_mnemonic(phrase, "").unwrap();
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        // Stable vector used by testnet premine / smoke-tx.
        assert_eq!(
            kp.address().to_hex(),
            "ff9ec96f09eb154d038a552ecae59c50204ea9a9"
        );
    }
}
