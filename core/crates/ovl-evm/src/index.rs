//! Receipt-derived token and NFT index.
//!
//! The index is reconstructable from canonical receipts. It is not a second
//! balance and it does not certify a wallet.

use serde_json::{json, Value};

use crate::world::EvmReceipt;

pub fn index_receipts(receipts: &[EvmReceipt]) -> Value {
    let transfer = topic("Transfer(address,address,uint256)");
    let transfer_single = topic("TransferSingle(address,address,address,uint256,uint256)");
    let mut erc20 = Vec::new();
    let mut erc721 = Vec::new();
    let mut erc1155 = Vec::new();
    for receipt in receipts {
        for log in &receipt.logs {
            if log.topics.first() == Some(&transfer) && log.topics.len() == 3 {
                erc20.push(json!({
                    "token": hex_addr(&log.address),
                    "from": topic_address(&log.topics[1]),
                    "to": topic_address(&log.topics[2]),
                    "value": hex_word_prefix(&log.data),
                    "transaction": hex_word(&receipt.hash),
                }));
            } else if log.topics.first() == Some(&transfer) && log.topics.len() == 4 {
                erc721.push(json!({
                    "token": hex_addr(&log.address),
                    "from": topic_address(&log.topics[1]),
                    "to": topic_address(&log.topics[2]),
                    "id": hex_word(&log.topics[3]),
                    "transaction": hex_word(&receipt.hash),
                }));
            } else if log.topics.first() == Some(&transfer_single) {
                erc1155.push(json!({
                    "token": hex_addr(&log.address),
                    "operator": topic_address(&log.topics[1]),
                    "from": topic_address(&log.topics[2]),
                    "to": topic_address(&log.topics[3]),
                    "data": format!("0x{}", hex::encode(&log.data)),
                    "transaction": hex_word(&receipt.hash),
                }));
            }
        }
    }
    json!({
        "erc20": erc20,
        "erc721": erc721,
        "erc1155": erc1155,
    })
}

pub fn topic(signature: &str) -> [u8; 32] {
    alloy_primitives::keccak256(signature.as_bytes()).0
}

fn topic_address(topic: &[u8; 32]) -> String {
    hex_addr(topic[12..].try_into().expect("20 bytes"))
}

fn hex_addr(bytes: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn hex_word(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn hex_word_prefix(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}
