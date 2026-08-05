//! JSON-RPC method dispatch over an injected [`RpcBackend`].
//!
//! Transport (HTTP/TCP) lives in `agora-node`; this module only maps methods to
//! acceptance-aware state queries. Confirmations never use block color alone.

use agora_types::{AcceptanceBitmap, Address, Amount, Block, Hash, Transaction, TxConfirmation};
use serde_json::json;

use crate::methods::{BlockAcceptanceView, RpcMethod, RpcRequest, RpcResponse, TxAcceptanceView};
use crate::RpcError;

/// Node-facing state surface for RPC handlers.
pub trait RpcBackend {
    fn dag_tips(&self) -> Vec<Hash>;
    fn get_block(&self, hash: &Hash) -> Option<Block>;
    fn submit_transaction(&mut self, tx: Transaction) -> Result<Hash, String>;
    fn get_balance(&self, address: &Address) -> Amount;
    fn get_block_acceptance(&self, hash: &Hash) -> Option<(AcceptanceBitmap, u64, u64)>;
    fn get_tx_confirmation(&self, tx_id: &Hash, tip_blue_score: u64) -> TxConfirmation;
    fn tip_blue_score(&self) -> u64;
}

fn parse_method(name: &str) -> Option<RpcMethod> {
    match name {
        "agora_getDagTips" => Some(RpcMethod::GetDagTips),
        "agora_getBlock" => Some(RpcMethod::GetBlock),
        "agora_submitTransaction" => Some(RpcMethod::SubmitTransaction),
        "agora_getBalance" => Some(RpcMethod::GetBalance),
        "agora_getBlockAcceptance" => Some(RpcMethod::GetBlockAcceptance),
        "agora_getTxConfirmation" => Some(RpcMethod::GetTxConfirmation),
        _ => None,
    }
}

/// Dispatch one JSON-RPC style request against `backend`.
pub fn dispatch(req: &RpcRequest, backend: &mut dyn RpcBackend) -> Result<RpcResponse, RpcError> {
    let method =
        parse_method(&req.method).ok_or_else(|| RpcError::MethodNotFound(req.method.clone()))?;

    let result = match method {
        RpcMethod::GetDagTips => {
            let tips = backend.dag_tips();
            serde_json::to_value(tips).map_err(|e| RpcError::InvalidParams(e.to_string()))?
        }
        RpcMethod::GetBlock => {
            let hash: Hash = serde_json::from_value(req.params.clone())
                .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
            let block = backend
                .get_block(&hash)
                .ok_or_else(|| RpcError::InvalidParams("block not found".into()))?;
            serde_json::to_value(block).map_err(|e| RpcError::InvalidParams(e.to_string()))?
        }
        RpcMethod::SubmitTransaction => {
            let tx: Transaction = serde_json::from_value(req.params.clone())
                .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
            let id = backend
                .submit_transaction(tx)
                .map_err(RpcError::InvalidParams)?;
            serde_json::to_value(id).map_err(|e| RpcError::InvalidParams(e.to_string()))?
        }
        RpcMethod::GetBalance => {
            let address: Address = serde_json::from_value(req.params.clone())
                .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
            let bal = backend.get_balance(&address);
            json!({ "address": address, "balance": bal })
        }
        RpcMethod::GetBlockAcceptance => {
            let hash: Hash = serde_json::from_value(req.params.clone())
                .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
            let (bitmap, fees, reward) = backend
                .get_block_acceptance(&hash)
                .ok_or_else(|| RpcError::InvalidParams("acceptance not found".into()))?;
            let view = BlockAcceptanceView {
                block_hash: hash,
                bitmap,
                accepted_fees: fees,
                coinbase_reward: reward,
            };
            serde_json::to_value(view).map_err(|e| RpcError::InvalidParams(e.to_string()))?
        }
        RpcMethod::GetTxConfirmation => {
            let tx_id: Hash = serde_json::from_value(req.params.clone())
                .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
            let tip = backend.tip_blue_score();
            let confirmation = backend.get_tx_confirmation(&tx_id, tip);
            let view = TxAcceptanceView::from_confirmation(tx_id, confirmation);
            serde_json::to_value(view).map_err(|e| RpcError::InvalidParams(e.to_string()))?
        }
    };

    Ok(RpcResponse { result })
}

#[cfg(test)]
mod tests {
    use agora_types::{AcceptanceBitmap, Amount, TxAcceptanceStatus};

    use super::*;

    struct Dummy;

    impl RpcBackend for Dummy {
        fn dag_tips(&self) -> Vec<Hash> {
            vec![Hash::ZERO]
        }
        fn get_block(&self, _: &Hash) -> Option<Block> {
            None
        }
        fn submit_transaction(&mut self, _: Transaction) -> Result<Hash, String> {
            Err("not wired".into())
        }
        fn get_balance(&self, _: &Address) -> Amount {
            Amount::ZERO
        }
        fn get_block_acceptance(&self, hash: &Hash) -> Option<(AcceptanceBitmap, u64, u64)> {
            (*hash == Hash::ZERO).then_some((AcceptanceBitmap::from_bools(&[true]), 1, 2))
        }
        fn get_tx_confirmation(&self, _: &Hash, tip: u64) -> TxConfirmation {
            TxConfirmation::accepted(Hash::ZERO, 1, tip)
        }
        fn tip_blue_score(&self) -> u64 {
            5
        }
    }

    #[test]
    fn dispatches_tips_and_confirmation() {
        let mut backend = Dummy;
        let tips = dispatch(
            &RpcRequest {
                method: "agora_getDagTips".into(),
                params: json!(null),
            },
            &mut backend,
        )
        .unwrap();
        assert!(tips.result.is_array());

        let conf = dispatch(
            &RpcRequest {
                method: "agora_getTxConfirmation".into(),
                params: serde_json::to_value(Hash::ZERO).unwrap(),
            },
            &mut backend,
        )
        .unwrap();
        let view: TxAcceptanceView = serde_json::from_value(conf.result).unwrap();
        assert_eq!(view.status, TxAcceptanceStatus::Accepted);
        assert_eq!(view.confirmation.confirmations, 4);
    }
}
