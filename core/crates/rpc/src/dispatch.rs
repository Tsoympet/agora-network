use agora_types::{
    AccountTransfer, Address, Amount, Block, DrcAccountPolicyTx, DrcCheckCancelTx, DrcCheckCashTx,
    DrcCheckCreateTx, DrcDepositPreauthTx, DrcEscrowCancelTx, DrcEscrowCreateTx, DrcEscrowFinishTx,
    DrcPaymentChannelClaimTx, DrcPaymentChannelCloseTx, DrcPaymentChannelCreateTx,
    DrcPaymentChannelFundTx, DrcPaymentReceipt, DrcPaymentTx, DrcRegularKeyTx, DrcSignerListTx,
    DrcTicketCreateTx, Hash, OvlExecutionTx, Transaction,
};
use serde_json::{json, Value};

use crate::backend::RpcBackend;
use crate::error::RpcError;
use crate::methods::{RpcMethod, RpcRequest, RpcResponse};

/// Dispatches JSON-RPC style requests against an [`RpcBackend`].
#[derive(Debug)]
pub struct RpcDispatcher<B: RpcBackend> {
    backend: B,
}

impl<B: RpcBackend> RpcDispatcher<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn handle(&mut self, req: RpcRequest) -> RpcResponse {
        let id = req.id.clone();
        match self.dispatch(&req) {
            Ok(result) => RpcResponse::ok(id, result),
            Err(err) => RpcResponse::err(id, &err),
        }
    }

    fn dispatch(&mut self, req: &RpcRequest) -> Result<Value, RpcError> {
        let method = RpcMethod::parse(&req.method)
            .ok_or_else(|| RpcError::MethodNotFound(req.method.clone()))?;
        match method {
            RpcMethod::GetDagTips => {
                let tips: Vec<String> = self
                    .backend
                    .dag_tips()
                    .into_iter()
                    .map(|h| h.to_hex())
                    .collect();
                Ok(json!(tips))
            }
            RpcMethod::GetBlock => {
                let hash = param_hash(&req.params, "hash")?;
                let block = self
                    .backend
                    .get_block(&hash)
                    .ok_or_else(|| RpcError::NotFound(hash.to_hex()))?;
                Ok(block_to_explorer_json(&block))
            }
            RpcMethod::GetTransaction => {
                let tx_id = param_hash(&req.params, "tx_id")?;
                let lookup = self.backend.get_transaction(&tx_id)?;
                Ok(tx_lookup_to_json(&lookup))
            }
            RpcMethod::GetMempool => {
                let limit = optional_limit(&req.params, 128)?;
                let entries = self.backend.get_mempool(limit)?;
                Ok(json!({
                    "count": entries.len(),
                    "transactions": entries.iter().map(mempool_entry_to_json).collect::<Vec<_>>(),
                }))
            }
            RpcMethod::GetNodeInfo => {
                let info = self.backend.get_node_info()?;
                Ok(node_info_to_json(&info))
            }
            RpcMethod::EstimateFee => {
                let fee = self.backend.estimate_fee()?;
                Ok(json!({
                    "min_relay_fee": fee.min_relay_fee,
                    "suggested_fee": fee.suggested_fee,
                }))
            }
            RpcMethod::SubmitTransaction => {
                let raw = tx_param(&req.params)?;
                let tx: Transaction = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_transaction(tx)?;
                Ok(json!({ "tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitAccountTransfer => {
                let raw = req
                    .params
                    .get("account_transfer")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: AccountTransfer = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_account_transfer(tx)?;
                Ok(json!({ "account_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitOvlExecution => {
                let raw = req
                    .params
                    .get("execution")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: OvlExecutionTx = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_ovl_execution(tx)?;
                Ok(json!({ "execution_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcPayment => {
                let raw = req
                    .params
                    .get("payment")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcPaymentTx = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                tx.validate_envelope_version()
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_drc_payment(tx)?;
                Ok(json!({ "payment_id": id.to_hex() }))
            }
            RpcMethod::GetDrcPayment => {
                let payment_id = param_hash(&req.params, "payment_id")?;
                let receipt = self.backend.get_drc_payment(&payment_id)?;
                Ok(json!({
                    "payment_id": payment_id.to_hex(),
                    "status": if receipt.is_some() { "settled" } else { "unknown" },
                    "receipt": receipt.as_ref().map(drc_payment_receipt_to_json),
                }))
            }
            RpcMethod::GetDrcPaymentByInvoice => {
                let (recipient, invoice_id) = drc_invoice_params(&req.params)?;
                let receipt = self
                    .backend
                    .get_drc_payment_by_invoice(&recipient, &invoice_id)?;
                Ok(json!({
                    "recipient": recipient.to_bech32(),
                    "invoice_id": invoice_id.to_hex(),
                    "payment_id": receipt.as_ref().map(|receipt| receipt.payment_id.to_hex()),
                    "status": if receipt.is_some() { "settled" } else { "unknown" },
                    "receipt": receipt.as_ref().map(drc_payment_receipt_to_json),
                }))
            }
            RpcMethod::SubmitDrcAccountPolicy => {
                let raw = req
                    .params
                    .get("policy")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcAccountPolicyTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_account_policy(tx)?;
                Ok(json!({ "policy_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcAccountPolicy => {
                let account = param_address(&req.params, "account")?;
                match self.backend.get_drc_account_policy(&account)? {
                    Some((policy, nonce)) => Ok(json!({
                        "account": account.to_bech32(),
                        "status": "known",
                        "policy": {
                            "version": policy.version,
                            "require_destination_tag": policy.require_destination_tag,
                            "deposit_auth_required": policy.deposit_auth_required,
                            "master_key_disabled": policy.master_key_disabled,
                        },
                        "account_nonce": nonce,
                    })),
                    None => Ok(json!({
                        "account": account.to_bech32(),
                        "status": "unknown",
                        "policy": null,
                        "account_nonce": null,
                    })),
                }
            }
            RpcMethod::SubmitDrcDepositPreauth => {
                let raw = req
                    .params
                    .get("preauth")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcDepositPreauthTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_deposit_preauth(tx)?;
                Ok(json!({ "preauth_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcDepositPreauth => {
                let (owner, authorized_source) = drc_deposit_preauth_params(&req.params)?;
                match self
                    .backend
                    .get_drc_deposit_preauth(&owner, &authorized_source)?
                {
                    Some(status) => Ok(json!({
                        "owner": owner.to_bech32(),
                        "authorized_source": authorized_source.to_bech32(),
                        "status": "known",
                        "preauthorized": status.preauthorized,
                        "deposit_auth_required": status.deposit_auth_required,
                        "deposit_authorized": status.deposit_authorized,
                    })),
                    None => Ok(json!({
                        "owner": owner.to_bech32(),
                        "authorized_source": authorized_source.to_bech32(),
                        "status": "unknown",
                        "preauthorized": null,
                        "deposit_auth_required": null,
                        "deposit_authorized": null,
                    })),
                }
            }
            RpcMethod::SubmitDrcRegularKey => {
                let raw = req
                    .params
                    .get("regular_key")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcRegularKeyTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_regular_key(tx)?;
                Ok(json!({ "regular_key_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcAccountKeys => {
                let account = param_address(&req.params, "account")?;
                match self.backend.get_drc_account_keys(&account)? {
                    Some((regular_key, nonce)) => Ok(json!({
                        "account": account.to_bech32(),
                        "status": "known",
                        "regular_key": regular_key.map(|key| key.to_bech32()),
                        "account_nonce": nonce,
                    })),
                    None => Ok(json!({
                        "account": account.to_bech32(),
                        "status": "unknown",
                        "regular_key": null,
                        "account_nonce": null,
                    })),
                }
            }
            RpcMethod::SubmitDrcSignerList => {
                let raw = req
                    .params
                    .get("signer_list")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcSignerListTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_signer_list(tx)?;
                Ok(json!({ "signer_list_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcTicketCreate => {
                let raw = req
                    .params
                    .get("ticket_create")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcTicketCreateTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_ticket_create(tx)?;
                Ok(json!({ "ticket_create_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcTicket => {
                let (owner, ticket_sequence) = drc_ticket_params(&req.params)?;
                self.backend.get_drc_ticket(&owner, ticket_sequence)
            }
            RpcMethod::SubmitDrcEscrowCreate => {
                let raw = req
                    .params
                    .get("escrow_create")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcEscrowCreateTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_escrow_create(tx)?;
                Ok(json!({ "escrow_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcEscrowFinish => {
                let raw = req
                    .params
                    .get("escrow_finish")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcEscrowFinishTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_escrow_finish(tx)?;
                Ok(json!({ "finish_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcEscrowCancel => {
                let raw = req
                    .params
                    .get("escrow_cancel")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcEscrowCancelTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_escrow_cancel(tx)?;
                Ok(json!({ "cancel_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcEscrow => {
                let escrow_id = param_hash(&req.params, "escrow_id")?;
                self.backend.get_drc_escrow(&escrow_id)
            }
            RpcMethod::GetDrcEscrowReceipt => {
                let escrow_id = param_hash(&req.params, "escrow_id")?;
                self.backend.get_drc_escrow_receipt(&escrow_id)
            }
            RpcMethod::SubmitDrcCheckCreate => {
                let raw = req
                    .params
                    .get("check_create")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcCheckCreateTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_check_create(tx)?;
                Ok(json!({ "check_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcCheckCash => {
                let raw = req
                    .params
                    .get("check_cash")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcCheckCashTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_check_cash(tx)?;
                Ok(json!({ "cash_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcCheckCancel => {
                let raw = req
                    .params
                    .get("check_cancel")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcCheckCancelTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_check_cancel(tx)?;
                Ok(json!({ "cancel_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcCheck => {
                let check_id = param_hash(&req.params, "check_id")?;
                self.backend.get_drc_check(&check_id)
            }
            RpcMethod::GetDrcCheckReceipt => {
                let check_id = param_hash(&req.params, "check_id")?;
                self.backend.get_drc_check_receipt(&check_id)
            }
            RpcMethod::SubmitDrcPaymentChannelCreate => {
                let raw = req
                    .params
                    .get("payment_channel_create")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcPaymentChannelCreateTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_payment_channel_create(tx)?;
                Ok(json!({ "channel_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcPaymentChannelFund => {
                let raw = req
                    .params
                    .get("payment_channel_fund")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcPaymentChannelFundTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_payment_channel_fund(tx)?;
                Ok(json!({ "fund_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcPaymentChannelClaim => {
                let raw = req
                    .params
                    .get("payment_channel_claim")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcPaymentChannelClaimTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_payment_channel_claim(tx)?;
                Ok(json!({ "claim_tx_id": id.to_hex() }))
            }
            RpcMethod::SubmitDrcPaymentChannelClose => {
                let raw = req
                    .params
                    .get("payment_channel_close")
                    .cloned()
                    .unwrap_or_else(|| req.params.clone());
                let tx: DrcPaymentChannelCloseTx = serde_json::from_value(raw)
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                tx.validate_structure()
                    .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
                let id = self.backend.submit_drc_payment_channel_close(tx)?;
                Ok(json!({ "close_tx_id": id.to_hex() }))
            }
            RpcMethod::GetDrcPaymentChannel => {
                let channel_id = param_hash(&req.params, "channel_id")?;
                self.backend.get_drc_payment_channel(&channel_id)
            }
            RpcMethod::GetDrcPaymentChannelReceipt => {
                let channel_id = param_hash(&req.params, "channel_id")?;
                self.backend.get_drc_payment_channel_receipt(&channel_id)
            }
            RpcMethod::GetDrcPaymentChannelFundEvent => {
                let fund_tx_id = param_hash(&req.params, "fund_tx_id")?;
                self.backend.get_drc_payment_channel_fund_event(&fund_tx_id)
            }
            RpcMethod::GetDrcPaymentChannelClaimEvent => {
                let claim_tx_id = param_hash(&req.params, "claim_tx_id")?;
                self.backend
                    .get_drc_payment_channel_claim_event(&claim_tx_id)
            }
            RpcMethod::GetDrcPaymentChannelScheduleEvent => {
                let close_tx_id = param_hash(&req.params, "close_tx_id")?;
                self.backend
                    .get_drc_payment_channel_schedule_event(&close_tx_id)
            }
            RpcMethod::VerifyDrcPaymentChannelClaim => {
                let channel_id = param_hash(&req.params, "channel_id")?;
                let cumulative_authorized = param_amount(&req.params, "cumulative_authorized")?;
                let channel_claim_signature =
                    param_hex_bytes(&req.params, "channel_claim_signature")?;
                self.backend.verify_drc_payment_channel_claim(
                    &channel_id,
                    cumulative_authorized,
                    &channel_claim_signature,
                )
            }
            RpcMethod::GetDrcAccountSignerList => {
                let account = param_address(&req.params, "account")?;
                match self.backend.get_drc_account_signer_list(&account)? {
                    Some((quorum, entry_count, nonce)) => Ok(json!({
                        "account": account.to_bech32(),
                        "status": "known",
                        "quorum": quorum,
                        "entry_count": entry_count,
                        "account_nonce": nonce,
                    })),
                    None => Ok(json!({
                        "account": account.to_bech32(),
                        "status": "unknown",
                        "quorum": null,
                        "entry_count": null,
                        "account_nonce": null,
                    })),
                }
            }
            RpcMethod::GetBalance => {
                let address = param_address(&req.params, "address")?;
                let bal = self.backend.get_balance(&address);
                Ok(json!({
                    "address": address.to_bech32(),
                    "balance": bal.as_base_units(),
                }))
            }
            RpcMethod::GetUtxos => {
                let address = param_address(&req.params, "address")?;
                let utxos = self.backend.get_utxos(&address)?;
                Ok(json!({
                    "address": address.to_bech32(),
                    "utxos": utxos.iter().map(|u| json!({
                        "tx_id": u.outpoint.tx_id.to_hex(),
                        "index": u.outpoint.index,
                        "value": u.value.as_base_units(),
                    })).collect::<Vec<_>>(),
                }))
            }
            RpcMethod::FundAddress => {
                let address = param_address(&req.params, "address")?;
                let amount = param_amount(&req.params, "amount")?;
                let bal = self.backend.fund_address(address, amount)?;
                Ok(json!({
                    "address": address.to_bech32(),
                    "balance": bal.as_base_units(),
                }))
            }
            RpcMethod::GetBlockTemplate => {
                let block = self.backend.get_block_template()?;
                let randomx_epoch = self.backend.randomx_epoch(&block.header.parents);
                // Wrap the block so miners receive the blue-score–anchored RandomX epoch.
                // Native serde shape (`Hash` as byte arrays) is preserved under `block`.
                let block_value =
                    serde_json::to_value(&block).map_err(|e| RpcError::Internal(e.to_string()))?;
                Ok(json!({
                    "block": block_value,
                    "randomx_epoch": randomx_epoch,
                }))
            }
            RpcMethod::SubmitBlock => {
                let raw = block_param(&req.params)?;
                let block: Block = serde_json::from_value(raw)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let id = self.backend.submit_block(block)?;
                Ok(json!({ "block_id": id.to_hex() }))
            }
            RpcMethod::GetFinality => {
                let hash = param_hash(&req.params, "hash")?;
                self.backend.get_finality(&hash)
            }
            RpcMethod::GetFinalizedTip => self.backend.get_finalized_tip(),
            RpcMethod::SubmitAttestation => {
                let att = req
                    .params
                    .get("attestation")
                    .cloned()
                    .or_else(|| {
                        if req.params.is_object() {
                            Some(req.params.clone())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| RpcError::InvalidParams("missing attestation object".into()))?;
                self.backend.submit_attestation(att)
            }
            RpcMethod::GetValidatorSet => {
                let asset = param_string(&req.params, "asset")?;
                let epoch = optional_u64_opt(&req.params, "epoch")?;
                self.backend.get_validator_set(&asset, epoch)
            }
            RpcMethod::GetValidator => {
                let asset = param_string(&req.params, "asset")?;
                let operator = param_address(&req.params, "operator")?;
                self.backend.get_validator(&asset, &operator)
            }
            RpcMethod::GetRewardPool => {
                let asset = param_string(&req.params, "asset")?;
                self.backend.get_reward_pool(&asset)
            }
            RpcMethod::GetProtocolTreasuries => self.backend.get_protocol_treasuries(),
            RpcMethod::GetCommunityRegistry => {
                let limit = optional_limit(&req.params, 64)?;
                self.backend.get_community_registry(limit)
            }
            RpcMethod::SubmitStakeTx => {
                let stake_tx = req
                    .params
                    .get("stake_tx")
                    .cloned()
                    .or_else(|| {
                        if req.params.is_object() {
                            Some(req.params.clone())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| RpcError::InvalidParams("missing stake_tx object".into()))?;
                self.backend.submit_stake_tx(stake_tx)
            }
            RpcMethod::GetConstitution => self.backend.get_constitution(),
            RpcMethod::GetGovernance => self.backend.get_governance(),
            RpcMethod::ListProposals => {
                let limit = optional_limit(&req.params, 64)?;
                self.backend.list_proposals(limit)
            }
            RpcMethod::GetProposal => {
                let id = param_u64(&req.params, "id")?;
                self.backend.get_proposal(id)
            }
            RpcMethod::ListOffices => self.backend.list_offices(),
            RpcMethod::ListForumTopics => {
                let limit = optional_limit(&req.params, 64)?;
                self.backend.list_forum_topics(limit)
            }
            RpcMethod::SubmitProposal => {
                let author = param_address(&req.params, "author")?;
                let title = param_string(&req.params, "title")?;
                let summary = param_string(&req.params, "summary")?;
                let kind = param_proposal_kind(&req.params)?;
                let slot = optional_u64(&req.params, "slot", 0)?;
                self.backend
                    .submit_proposal(author, title, summary, kind, slot)
            }
            RpcMethod::DepositProposal => {
                let id = param_u64(&req.params, "id")?;
                let amount = param_u64(&req.params, "amount")?;
                self.backend.deposit_proposal(id, amount)
            }
            RpcMethod::OpenProposalVoting => {
                let id = param_u64(&req.params, "id")?;
                let slot = optional_u64(&req.params, "slot", 0)?;
                self.backend.open_proposal_voting(id, slot)
            }
            RpcMethod::CastGovVote => {
                let id = param_u64(&req.params, "id")?;
                let voter = param_address(&req.params, "voter")?;
                let choice = param_vote_choice(&req.params)?;
                let raw_balance = optional_u64(&req.params, "raw_balance", 0)?;
                let total_supply = optional_u64(&req.params, "total_supply", 1)?;
                self.backend
                    .cast_gov_vote(id, voter, choice, raw_balance, total_supply)
            }
            RpcMethod::TallyProposal => {
                let id = param_u64(&req.params, "id")?;
                self.backend.tally_proposal(id)
            }
            RpcMethod::EnterProposalTimelock => {
                let id = param_u64(&req.params, "id")?;
                let slot = optional_u64(&req.params, "slot", 0)?;
                self.backend.enter_proposal_timelock(id, slot)
            }
            RpcMethod::ExecuteProposal => {
                let id = param_u64(&req.params, "id")?;
                let slot = optional_u64(&req.params, "slot", 0)?;
                self.backend.execute_proposal(id, slot)
            }
            RpcMethod::PostForumTopic => {
                let author = param_address(&req.params, "author")?;
                let title = param_string(&req.params, "title")?;
                let body = param_string(&req.params, "body")?;
                let category = param_topic_category(&req.params)?;
                let slot = optional_u64(&req.params, "slot", 0)?;
                self.backend
                    .post_forum_topic(author, title, body, category, slot)
            }
            RpcMethod::AckConstitution => {
                let address = param_address(&req.params, "address")?;
                let slot = optional_u64(&req.params, "slot", 0)?;
                self.backend.ack_constitution(address, slot)
            }
            RpcMethod::SponsorProposal => {
                let id = param_u64(&req.params, "id")?;
                let who = param_address(&req.params, "who")?;
                self.backend.sponsor_proposal(id, who)
            }
            RpcMethod::AssentProposal => {
                let id = param_u64(&req.params, "id")?;
                let who = param_address(&req.params, "who")?;
                self.backend.assent_proposal(id, who)
            }
        }
    }
}

/// Hex-friendly block JSON for wallets / explorer (hashes as hex strings).
fn block_to_explorer_json(block: &Block) -> Value {
    let id = block.id().to_hex();
    let transactions: Vec<Value> = block.transactions.iter().map(tx_to_explorer_json).collect();
    json!({
        "id": id,
        "header": header_to_explorer_json(&block.header, Some(id)),
        "tx_count": block.transactions.len(),
        "account_transfer_count": block.account_transfers.len(),
        "stake_op_count": block.stake_ops.len(),
        "ovl_execution_count": block.ovl_executions.len(),
        "drc_payment_count": block.drc_payments.len(),
        "drc_account_policy_count": block.drc_account_policies.len(),
        "drc_deposit_preauth_count": block.drc_deposit_preauths.len(),
        "transactions": transactions,
    })
}

fn tx_to_explorer_json(tx: &Transaction) -> Value {
    json!({
        "tx_id": tx.tx_id().to_hex(),
        "version": tx.version,
        "inputs": tx.inputs.iter().map(|i| json!({
            "tx_id": i.previous_outpoint.tx_id.to_hex(),
            "index": i.previous_outpoint.index,
        })).collect::<Vec<_>>(),
        "outputs": tx.outputs.iter().map(|o| json!({
            "value": o.value.as_base_units(),
            "address": o.address.to_bech32(),
        })).collect::<Vec<_>>(),
        "nonce": tx.nonce,
        "is_coinbase": tx.inputs.is_empty(),
    })
}

fn tx_lookup_to_json(lookup: &crate::backend::TxLookup) -> Value {
    json!({
        "tx_id": lookup.tx_id.to_hex(),
        "status": lookup.status.as_str(),
        "block_id": lookup.block_id.map(|h| h.to_hex()),
        "index": lookup.index,
        "fee": lookup.fee,
        "confirmations": lookup.confirmations,
        "acceptance": lookup.acceptance,
        "transaction": lookup.transaction.as_ref().map(tx_to_explorer_json),
    })
}

fn mempool_entry_to_json(entry: &crate::backend::MempoolEntry) -> Value {
    json!({
        "tx_id": entry.tx_id.to_hex(),
        "fee": entry.fee,
        "transaction": tx_to_explorer_json(&entry.transaction),
    })
}

fn drc_payment_receipt_to_json(receipt: &DrcPaymentReceipt) -> Value {
    let mut receipt_json = json!({
        "version": receipt.version,
        "payment_version": receipt.payment_version,
        "result": receipt.result.as_str(),
        "from": receipt.from.to_bech32(),
        "to": receipt.to.to_bech32(),
        "requested_amount": receipt.requested_amount.as_base_units(),
        "delivered_amount": receipt.delivered_amount.as_base_units(),
        "fee_paid": receipt.fee_paid.as_base_units(),
        "source_tag": receipt.source_tag,
        "destination_tag": receipt.destination_tag,
        "invoice_id": receipt.invoice_id.to_hex(),
    });
    if let Some(cutoff) = receipt.last_valid_blue_score {
        receipt_json
            .as_object_mut()
            .expect("receipt object")
            .insert("last_valid_blue_score".into(), json!(cutoff));
    }
    receipt_json
}

fn node_info_to_json(info: &crate::backend::NodeInfo) -> Value {
    json!({
        "network": info.network,
        "version": info.version,
        "peer_id": info.peer_id,
        "connected_peers": info.connected_peers,
        "tip_count": info.tip_count,
        "mempool_count": info.mempool_count,
        "pow_algorithm": info.pow_algorithm,
        "bits": info.bits,
        "archival": info.archival,
        "hot_window": info.hot_window,
        "allow_fund": info.allow_fund,
        "miner_address": info.miner_address,
        "genesis_hash": info.genesis_hash,
        "chain_id": info.chain_id,
        "min_relay_fee": info.min_relay_fee,
    })
}

/// Optional `{ "limit": N }` / `[N]` / bare number; default when omitted.
fn optional_limit(params: &Value, default: usize) -> Result<usize, RpcError> {
    if params.is_null() {
        return Ok(default);
    }
    if let Some(arr) = params.as_array() {
        if arr.is_empty() {
            return Ok(default);
        }
        return parse_limit_value(&arr[0], default);
    }
    if let Some(obj) = params.as_object() {
        if obj.is_empty() {
            return Ok(default);
        }
        if let Some(v) = obj.get("limit") {
            return parse_limit_value(v, default);
        }
        return Ok(default);
    }
    parse_limit_value(params, default)
}

fn parse_limit_value(v: &Value, default: usize) -> Result<usize, RpcError> {
    if v.is_null() {
        return Ok(default);
    }
    let n = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| RpcError::InvalidParams("`limit` must be u64".into()))?;
    if n == 0 {
        return Ok(default);
    }
    Ok(n.min(10_000) as usize)
}

fn header_to_explorer_json(header: &agora_types::BlockHeader, id: Option<String>) -> Value {
    let mut obj = json!({
        "version": header.version,
        "parents": header.parents.iter().map(|p| p.to_hex()).collect::<Vec<_>>(),
        "timestamp_ms": header.timestamp_ms,
        "bits": header.bits,
        "nonce": header.nonce,
        "tx_root": header.tx_root.to_hex(),
    });
    if let Some(id) = id {
        obj.as_object_mut()
            .expect("object")
            .insert("id".into(), json!(id));
    }
    obj
}

fn single_or_named(params: &Value, key: &str) -> Result<Value, RpcError> {
    if let Some(obj) = params.as_object() {
        return obj
            .get(key)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")));
    }
    if let Some(arr) = params.as_array() {
        return arr
            .first()
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")));
    }
    // Allow bare value.
    if !params.is_null() {
        return Ok(params.clone());
    }
    Err(RpcError::InvalidParams(format!("missing `{key}`")))
}

/// Accept `{ "tx": {...} }`, `[tx]`, or a bare transaction object.
fn tx_param(params: &Value) -> Result<Value, RpcError> {
    if let Some(obj) = params.as_object() {
        if let Some(tx) = obj.get("tx") {
            return Ok(tx.clone());
        }
        if obj.contains_key("version") && obj.contains_key("outputs") {
            return Ok(params.clone());
        }
        return Err(RpcError::InvalidParams("missing `tx`".into()));
    }
    single_or_named(params, "tx")
}

/// Accept `{ "block": {...} }`, `[block]`, or a bare block object.
fn block_param(params: &Value) -> Result<Value, RpcError> {
    if let Some(obj) = params.as_object() {
        if let Some(block) = obj.get("block") {
            return Ok(block.clone());
        }
        if obj.contains_key("header") {
            return Ok(params.clone());
        }
        return Err(RpcError::InvalidParams("missing `block`".into()));
    }
    single_or_named(params, "block")
}

fn param_hash(params: &Value, key: &str) -> Result<Hash, RpcError> {
    let v = single_or_named(params, key)?;
    parse_hash_value(&v, key)
}

fn parse_hash_value(v: &Value, key: &str) -> Result<Hash, RpcError> {
    let s = v
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be hex string")))?;
    Hash::from_hex(s).ok_or_else(|| RpcError::InvalidParams(format!("invalid hash `{s}`")))
}

fn param_hex_bytes(params: &Value, key: &str) -> Result<Vec<u8>, RpcError> {
    let v = single_or_named(params, key)?;
    let s = v
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be hex string")))?;
    hex::decode(s.trim_start_matches("0x"))
        .map_err(|_| RpcError::InvalidParams(format!("invalid hex `{key}`")))
}

fn param_address(params: &Value, key: &str) -> Result<Address, RpcError> {
    let v = single_or_named(params, key)?;
    parse_address_value(&v, key)
}

fn parse_address_value(v: &Value, key: &str) -> Result<Address, RpcError> {
    let s = v
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be bech32 or hex string")))?;
    Address::parse(s).ok_or_else(|| RpcError::InvalidParams(format!("invalid address `{s}`")))
}

fn drc_invoice_params(params: &Value) -> Result<(Address, Hash), RpcError> {
    let (recipient, invoice_id) = if let Some(obj) = params.as_object() {
        let recipient = obj
            .get("recipient")
            .ok_or_else(|| RpcError::InvalidParams("missing `recipient`".into()))?;
        let invoice_id = obj
            .get("invoice_id")
            .ok_or_else(|| RpcError::InvalidParams("missing `invoice_id`".into()))?;
        (recipient, invoice_id)
    } else if let Some(arr) = params.as_array() {
        if arr.len() != 2 {
            return Err(RpcError::InvalidParams(
                "expected `[recipient, invoice_id]`".into(),
            ));
        }
        (&arr[0], &arr[1])
    } else {
        return Err(RpcError::InvalidParams(
            "expected `{recipient, invoice_id}` or `[recipient, invoice_id]`".into(),
        ));
    };
    Ok((
        parse_address_value(recipient, "recipient")?,
        parse_hash_value(invoice_id, "invoice_id")?,
    ))
}

fn drc_ticket_params(params: &Value) -> Result<(Address, u64), RpcError> {
    let (owner, ticket_sequence) = if let Some(obj) = params.as_object() {
        let owner = obj
            .get("owner")
            .ok_or_else(|| RpcError::InvalidParams("missing `owner`".into()))?;
        let ticket_sequence = obj
            .get("ticket_sequence")
            .ok_or_else(|| RpcError::InvalidParams("missing `ticket_sequence`".into()))?;
        (owner, ticket_sequence)
    } else if let Some(arr) = params.as_array() {
        if arr.len() != 2 {
            return Err(RpcError::InvalidParams(
                "expected `[owner, ticket_sequence]`".into(),
            ));
        }
        (&arr[0], &arr[1])
    } else {
        return Err(RpcError::InvalidParams(
            "expected `{owner, ticket_sequence}` or `[owner, ticket_sequence]`".into(),
        ));
    };
    let sequence = ticket_sequence
        .as_u64()
        .or_else(|| ticket_sequence.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| RpcError::InvalidParams("`ticket_sequence` must be u64".into()))?;
    Ok((parse_address_value(owner, "owner")?, sequence))
}

fn drc_deposit_preauth_params(params: &Value) -> Result<(Address, Address), RpcError> {
    let (owner, authorized_source) = if let Some(obj) = params.as_object() {
        let owner = obj
            .get("owner")
            .ok_or_else(|| RpcError::InvalidParams("missing `owner`".into()))?;
        let authorized_source = obj
            .get("authorized_source")
            .ok_or_else(|| RpcError::InvalidParams("missing `authorized_source`".into()))?;
        (owner, authorized_source)
    } else if let Some(arr) = params.as_array() {
        if arr.len() != 2 {
            return Err(RpcError::InvalidParams(
                "expected `[owner, authorized_source]`".into(),
            ));
        }
        (&arr[0], &arr[1])
    } else {
        return Err(RpcError::InvalidParams(
            "expected `{owner, authorized_source}` or `[owner, authorized_source]`".into(),
        ));
    };
    Ok((
        parse_address_value(owner, "owner")?,
        parse_address_value(authorized_source, "authorized_source")?,
    ))
}

fn param_amount(params: &Value, key: &str) -> Result<Amount, RpcError> {
    // Support object `{address, amount}` or array `[address, amount]`.
    let amount_val = if let Some(obj) = params.as_object() {
        obj.get(key)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")))?
    } else if let Some(arr) = params.as_array() {
        arr.get(1)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")))?
    } else {
        return Err(RpcError::InvalidParams(format!("missing `{key}`")));
    };

    let units = amount_val
        .as_u64()
        .or_else(|| amount_val.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be u64")))?;
    Ok(Amount::from_base_units(units))
}

fn param_u64(params: &Value, key: &str) -> Result<u64, RpcError> {
    let v = if let Some(obj) = params.as_object() {
        obj.get(key)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")))?
    } else {
        single_or_named(params, key)?
    };
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be u64")))
}

fn optional_u64(params: &Value, key: &str, default: u64) -> Result<u64, RpcError> {
    Ok(optional_u64_opt(params, key)?.unwrap_or(default))
}

fn optional_u64_opt(params: &Value, key: &str) -> Result<Option<u64>, RpcError> {
    let Some(obj) = params.as_object() else {
        return Ok(None);
    };
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .map(Some)
            .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be u64"))),
    }
}

fn param_string(params: &Value, key: &str) -> Result<String, RpcError> {
    let v = if let Some(obj) = params.as_object() {
        obj.get(key)
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams(format!("missing `{key}`")))?
    } else {
        return Err(RpcError::InvalidParams(format!("missing `{key}`")));
    };
    v.as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| RpcError::InvalidParams(format!("`{key}` must be string")))
}

fn param_proposal_kind(params: &Value) -> Result<agora_governance::ProposalKind, RpcError> {
    let raw = if let Some(obj) = params.as_object() {
        obj.get("kind")
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams("missing `kind`".into()))?
    } else {
        return Err(RpcError::InvalidParams("missing `kind`".into()));
    };
    serde_json::from_value(raw).map_err(|e| RpcError::InvalidParams(format!("kind: {e}")))
}

fn param_vote_choice(params: &Value) -> Result<agora_governance::VoteChoice, RpcError> {
    let raw = if let Some(obj) = params.as_object() {
        obj.get("choice")
            .cloned()
            .ok_or_else(|| RpcError::InvalidParams("missing `choice`".into()))?
    } else {
        return Err(RpcError::InvalidParams("missing `choice`".into()));
    };
    serde_json::from_value(raw).map_err(|e| RpcError::InvalidParams(format!("choice: {e}")))
}

fn param_topic_category(params: &Value) -> Result<agora_governance::TopicCategory, RpcError> {
    let raw = if let Some(obj) = params.as_object() {
        obj.get("category")
            .cloned()
            .unwrap_or_else(|| json!("discussion"))
    } else {
        json!("discussion")
    };
    serde_json::from_value(raw).map_err(|e| RpcError::InvalidParams(format!("category: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::InMemoryBackend;
    use crate::methods::RpcMethod;
    use agora_types::{
        Amount, Block, BlockHeader, DrcAccountPolicy, DrcAccountPolicyTx, DrcPaymentTx,
        DrcTicketCreateTx, TxOut,
    };

    #[test]
    fn tips_balance_submit_fund() {
        let mut backend = InMemoryBackend::new();
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        let genesis_id = genesis.id();
        backend.insert_block(genesis);

        let mut rpc = RpcDispatcher::new(backend);
        let tips = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDagTips".into(),
            params: json!([]),
        });
        assert_eq!(tips.result.unwrap(), json!([genesis_id.to_hex()]));

        let addr = Address::from_hex("aabbccddeeff00112233445566778899aabbccdd").unwrap();
        let funded = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_fundAddress".into(),
            params: json!({"address": addr.to_bech32(), "amount": 500u64}),
        });
        let funded_res = funded.result.unwrap();
        assert_eq!(funded_res["balance"], json!(500));
        assert_eq!(funded_res["address"], json!(addr.to_bech32()));

        let bal = rpc.handle(RpcRequest {
            id: None,
            method: "agora_getBalance".into(),
            params: json!([addr.to_hex()]), // hex still accepted
        });
        let bal_res = bal.result.unwrap();
        assert_eq!(bal_res["balance"], json!(500));
        assert_eq!(bal_res["address"], json!(addr.to_bech32()));

        let utxos = rpc.handle(RpcRequest {
            id: Some(json!(21)),
            method: "agora_getUtxos".into(),
            params: json!({"address": addr.to_bech32()}),
        });
        let utxo_list = utxos.result.unwrap()["utxos"].as_array().unwrap().clone();
        assert_eq!(utxo_list.len(), 1);
        assert_eq!(utxo_list[0]["value"], json!(500));
        assert_eq!(utxo_list[0]["index"], json!(0));

        let tx = Transaction::unsigned(
            1,
            vec![],
            vec![TxOut {
                value: Amount::from_base_units(1),
                address: addr,
            }],
            1,
        );
        let submitted = rpc.handle(RpcRequest {
            id: Some(json!(3)),
            method: "agora_submitTransaction".into(),
            params: json!(tx.clone()),
        });
        let tx_id = submitted.result.unwrap()["tx_id"]
            .as_str()
            .unwrap()
            .to_string();

        let pending = rpc.handle(RpcRequest {
            id: Some(json!(31)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": tx_id}),
        });
        let pending_res = pending.result.unwrap();
        assert_eq!(pending_res["status"], json!("pending"));
        assert!(pending_res["transaction"].is_object());

        let pool = rpc.handle(RpcRequest {
            id: Some(json!(311)),
            method: "agora_getMempool".into(),
            params: json!([]),
        });
        let pool_res = pool.result.unwrap();
        assert_eq!(pool_res["count"], json!(1));
        assert_eq!(pool_res["transactions"][0]["tx_id"], json!(tx_id));

        let info = rpc.handle(RpcRequest {
            id: Some(json!(312)),
            method: "agora_getNodeInfo".into(),
            params: json!([]),
        });
        let info_res = info.result.unwrap();
        assert_eq!(info_res["network"], json!("dev"));
        assert_eq!(info_res["tip_count"], json!(1));
        assert_eq!(info_res["mempool_count"], json!(1));
        assert_eq!(info_res["archival"], json!(true));
        assert_eq!(info_res["genesis_hash"], json!(genesis_id.to_hex()));

        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(32)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": Hash::ZERO.to_hex()}),
        });
        assert_eq!(unknown.result.unwrap()["status"], json!("unknown"));

        let mined = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![genesis_id],
                timestamp_ms: 1,
                bits: 0,
                nonce: 1,
                tx_root: Block::compute_tx_root(std::slice::from_ref(&tx)),
            },
            transactions: vec![tx],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        let mined_id = mined.id();
        rpc.backend_mut().insert_block(mined);

        let confirmed = rpc.handle(RpcRequest {
            id: Some(json!(33)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": tx_id}),
        });
        let confirmed_res = confirmed.result.unwrap();
        assert_eq!(confirmed_res["status"], json!("confirmed"));
        assert_eq!(confirmed_res["block_id"], json!(mined_id.to_hex()));
        assert_eq!(confirmed_res["index"], json!(0));
        assert_eq!(confirmed_res["confirmations"], json!(1));

        // Child tip → parent tx gains a confirmation.
        let child = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![mined_id],
                timestamp_ms: 2,
                bits: 0,
                nonce: 2,
                tx_root: Block::compute_tx_root(&[]),
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        rpc.backend_mut().insert_block(child);
        let deeper = rpc.handle(RpcRequest {
            id: Some(json!(34)),
            method: "agora_getTransaction".into(),
            params: json!({"tx_id": tx_id}),
        });
        assert_eq!(deeper.result.unwrap()["confirmations"], json!(2));

        let block = rpc.handle(RpcRequest {
            id: Some(json!(4)),
            method: "agora_getBlock".into(),
            params: json!([genesis_id.to_hex()]),
        });
        let result = block.result.unwrap();
        assert_eq!(result["id"], json!(genesis_id.to_hex()));
        assert!(result["header"]["parents"].as_array().unwrap().is_empty());
        assert_eq!(result["tx_count"], json!(0));
        assert!(result["transactions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn drc_payment_queries_serialize_exact_receipts_and_isolate_invoices() {
        let payment = DrcPaymentTx::unsigned_v2(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(300),
            Amount::from_base_units(7),
            42,
            Some(84),
            Hash([9; 32]),
            5,
        );
        let receipt = DrcPaymentReceipt::delivered_exact(&payment);
        let payment_id = receipt.payment_id;
        let mut backend = InMemoryBackend::new();
        backend.insert_drc_payment_receipt(receipt);
        let second_payment = DrcPaymentTx::unsigned_v2(
            Address([3; 20]),
            Address([4; 20]),
            Amount::from_base_units(500),
            Amount::from_base_units(8),
            43,
            None,
            payment.invoice_id,
            6,
        );
        let second_receipt = DrcPaymentReceipt::delivered_exact(&second_payment);
        let second_payment_id = second_receipt.payment_id;
        backend.insert_drc_payment_receipt(second_receipt);
        let mut rpc = RpcDispatcher::new(backend);

        let settled = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDrcPayment".into(),
            params: json!({ "payment_id": payment_id.to_hex() }),
        });
        assert_eq!(
            settled.result.unwrap(),
            json!({
                "payment_id": payment_id.to_hex(),
                "status": "settled",
                "receipt": {
                    "version": 1,
                    "payment_version": 2,
                    "result": "delivered_exact",
                    "from": payment.from.to_bech32(),
                    "to": payment.to.to_bech32(),
                    "requested_amount": 300,
                    "delivered_amount": 300,
                    "fee_paid": 7,
                    "source_tag": 84,
                    "destination_tag": 42,
                    "invoice_id": payment.invoice_id.to_hex(),
                }
            })
        );

        let by_invoice = rpc.handle(RpcRequest {
            id: Some(json!(11)),
            method: "agora_getDrcPaymentByInvoice".into(),
            params: json!({
                "recipient": payment.to.to_hex(),
                "invoice_id": payment.invoice_id.to_hex(),
            }),
        });
        let by_invoice_result = by_invoice.result.unwrap();
        assert_eq!(by_invoice_result["recipient"], payment.to.to_bech32());
        assert_eq!(by_invoice_result["invoice_id"], payment.invoice_id.to_hex());
        assert_eq!(by_invoice_result["payment_id"], payment_id.to_hex());
        assert_eq!(by_invoice_result["status"], "settled");
        assert_eq!(by_invoice_result["receipt"]["source_tag"], 84);
        assert_eq!(by_invoice_result["receipt"]["destination_tag"], 42);
        assert!(by_invoice_result["receipt"].get("signature").is_none());
        assert!(by_invoice_result["receipt"].get("public_key").is_none());

        let same_invoice_other_recipient = rpc.handle(RpcRequest {
            id: Some(json!(12)),
            method: "agora_getDrcPaymentByInvoice".into(),
            params: json!([
                second_payment.to.to_bech32(),
                second_payment.invoice_id.to_hex()
            ]),
        });
        let other_result = same_invoice_other_recipient.result.unwrap();
        assert_eq!(other_result["payment_id"], second_payment_id.to_hex());
        assert_eq!(other_result["receipt"]["to"], second_payment.to.to_bech32());
        assert_eq!(other_result["receipt"]["source_tag"], Value::Null);
        assert_eq!(other_result["receipt"]["destination_tag"], 43);
        assert_ne!(other_result["payment_id"], by_invoice_result["payment_id"]);

        let unknown_id = Hash([3; 32]);
        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_getDrcPayment".into(),
            params: json!([unknown_id.to_hex()]),
        });
        assert_eq!(
            unknown.result.unwrap(),
            json!({
                "payment_id": unknown_id.to_hex(),
                "status": "unknown",
                "receipt": null,
            })
        );

        let wrong_recipient = Address([5; 20]);
        let unknown_invoice = rpc.handle(RpcRequest {
            id: Some(json!(21)),
            method: "agora_getDrcPaymentByInvoice".into(),
            params: json!({
                "recipient": wrong_recipient.to_bech32(),
                "invoice_id": payment.invoice_id.to_hex(),
            }),
        });
        assert_eq!(
            unknown_invoice.result.unwrap(),
            json!({
                "recipient": wrong_recipient.to_bech32(),
                "invoice_id": payment.invoice_id.to_hex(),
                "payment_id": null,
                "status": "unknown",
                "receipt": null,
            })
        );

        let absent_invoice = Hash([6; 32]);
        let absent = rpc.handle(RpcRequest {
            id: Some(json!(22)),
            method: "agora_getDrcPaymentByInvoice".into(),
            params: json!({
                "recipient": payment.to.to_bech32(),
                "invoice_id": absent_invoice.to_hex(),
            }),
        });
        assert_eq!(absent.result.unwrap()["status"], "unknown");

        let no_invoice = rpc.handle(RpcRequest {
            id: Some(json!(23)),
            method: "agora_getDrcPaymentByInvoice".into(),
            params: json!({
                "recipient": payment.to.to_bech32(),
                "invoice_id": Hash::ZERO.to_hex(),
            }),
        });
        let no_invoice_result = no_invoice.result.unwrap();
        assert_eq!(no_invoice_result["status"], "unknown");
        assert_eq!(no_invoice_result["payment_id"], Value::Null);
        assert_eq!(no_invoice_result["receipt"], Value::Null);

        for malformed in ["abcd".to_string(), "g".repeat(64)] {
            let response = rpc.handle(RpcRequest {
                id: Some(json!(3)),
                method: "agora_getDrcPayment".into(),
                params: json!({ "payment_id": malformed }),
            });
            let error = response.error.unwrap();
            assert_eq!(error.code, -32602);
            assert!(error.message.contains("invalid hash"));
        }

        for params in [
            json!({
                "recipient": "abcd",
                "invoice_id": payment.invoice_id.to_hex(),
            }),
            json!({
                "recipient": payment.to.to_bech32(),
                "invoice_id": "abcd",
            }),
            json!([payment.to.to_bech32()]),
            json!({
                "recipient": payment.to.to_bech32(),
                "invoice_id": 7,
            }),
        ] {
            let response = rpc.handle(RpcRequest {
                id: Some(json!(4)),
                method: "agora_getDrcPaymentByInvoice".into(),
                params,
            });
            assert_eq!(response.error.unwrap().code, -32602);
        }
    }

    #[test]
    fn drc_payment_v4_receipt_includes_last_valid_blue_score_only_when_committed() {
        let payment = DrcPaymentTx::unsigned_v4(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(100),
            Amount::from_base_units(2),
            Some(0),
            None,
            Hash([4; 32]),
            0,
            Some(99),
        );
        let receipt = DrcPaymentReceipt::delivered_exact(&payment);
        let payment_id = receipt.payment_id;
        let mut backend = InMemoryBackend::new();
        backend.insert_drc_payment_receipt(receipt);
        let mut rpc = RpcDispatcher::new(backend);
        let response = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDrcPayment".into(),
            params: json!({ "payment_id": payment_id.to_hex() }),
        });
        let result = response.result.unwrap();
        assert_eq!(result["receipt"]["last_valid_blue_score"], 99);
        assert_eq!(result["receipt"]["payment_version"], 4);

        let legacy_receipt = DrcPaymentReceipt::delivered_exact(&DrcPaymentTx::unsigned_v3(
            Address([3; 20]),
            Address([4; 20]),
            Amount::from_base_units(1),
            Amount::ZERO,
            None,
            None,
            Hash::ZERO,
            0,
        ));
        let legacy_id = legacy_receipt.payment_id;
        let mut backend = InMemoryBackend::new();
        backend.insert_drc_payment_receipt(legacy_receipt);
        let mut rpc = RpcDispatcher::new(backend);
        let response = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_getDrcPayment".into(),
            params: json!({ "payment_id": legacy_id.to_hex() }),
        });
        assert!(response.result.unwrap()["receipt"]
            .get("last_valid_blue_score")
            .is_none());
    }

    #[test]
    fn drc_account_policy_rpc_handles_known_unknown_and_malformed_params() {
        let known = Address([7; 20]);
        let unknown = Address([8; 20]);
        let mut backend = InMemoryBackend::new();
        backend.insert_drc_account_policy(
            known,
            DrcAccountPolicy {
                version: agora_types::DRC_ACCOUNT_POLICY_STATE_VERSION,
                require_destination_tag: true,
                deposit_auth_required: true,
                master_key_disabled: false,
            },
            9,
        );
        let mut rpc = RpcDispatcher::new(backend);

        let response = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDrcAccountPolicy".into(),
            params: json!({ "account": known.to_hex() }),
        });
        assert_eq!(
            response.result.unwrap(),
            json!({
                "account": known.to_bech32(),
                "status": "known",
                "policy": {
                    "version": agora_types::DRC_ACCOUNT_POLICY_STATE_VERSION,
                    "require_destination_tag": true,
                    "deposit_auth_required": true,
                    "master_key_disabled": false,
                },
                "account_nonce": 9,
            })
        );

        let response = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_getDrcAccountPolicy".into(),
            params: json!([unknown.to_bech32()]),
        });
        assert_eq!(
            response.result.unwrap(),
            json!({
                "account": unknown.to_bech32(),
                "status": "unknown",
                "policy": null,
                "account_nonce": null,
            })
        );

        for params in [json!({}), json!({"account": 7}), json!({"account": "abcd"})] {
            let response = rpc.handle(RpcRequest {
                id: Some(json!(3)),
                method: "agora_getDrcAccountPolicy".into(),
                params,
            });
            assert_eq!(response.error.unwrap().code, -32602);
        }

        let malformed_submit = rpc.handle(RpcRequest {
            id: Some(json!(4)),
            method: "agora_submitDrcAccountPolicy".into(),
            params: json!({ "policy": { "version": 1 } }),
        });
        assert_eq!(malformed_submit.error.unwrap().code, -32602);

        let valid =
            DrcAccountPolicyTx::set_require_destination_tag(known, Amount::from_base_units(1), 9);
        let rejected = rpc.handle(RpcRequest {
            id: Some(json!(5)),
            method: "agora_submitDrcAccountPolicy".into(),
            params: json!({ "policy": valid }),
        });
        assert_eq!(rejected.error.unwrap().code, -32001);
    }

    #[test]
    fn drc_deposit_preauth_rpc_handles_known_unknown_and_malformed_params() {
        let owner = Address([7; 20]);
        let source = Address([8; 20]);
        let unknown_source = Address([9; 20]);
        let mut backend = InMemoryBackend::new();
        backend.insert_drc_deposit_preauth(
            owner,
            source,
            crate::backend::DrcDepositPreauthStatus {
                preauthorized: true,
                deposit_auth_required: true,
                deposit_authorized: true,
            },
        );
        let mut rpc = RpcDispatcher::new(backend);

        let known = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDrcDepositPreauth".into(),
            params: json!({
                "owner": owner.to_hex(),
                "authorized_source": source.to_bech32(),
            }),
        });
        assert_eq!(
            known.result.unwrap(),
            json!({
                "owner": owner.to_bech32(),
                "authorized_source": source.to_bech32(),
                "status": "known",
                "preauthorized": true,
                "deposit_auth_required": true,
                "deposit_authorized": true,
            })
        );

        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_getDrcDepositPreauth".into(),
            params: json!([owner.to_bech32(), unknown_source.to_hex()]),
        });
        assert_eq!(
            unknown.result.unwrap(),
            json!({
                "owner": owner.to_bech32(),
                "authorized_source": unknown_source.to_bech32(),
                "status": "unknown",
                "preauthorized": null,
                "deposit_auth_required": null,
                "deposit_authorized": null,
            })
        );

        for params in [
            json!({}),
            json!({"owner": owner.to_hex()}),
            json!({"owner": 7, "authorized_source": source.to_hex()}),
            json!({"owner": owner.to_hex(), "authorized_source": "abcd"}),
            json!([owner.to_hex()]),
        ] {
            let response = rpc.handle(RpcRequest {
                id: Some(json!(3)),
                method: "agora_getDrcDepositPreauth".into(),
                params,
            });
            assert_eq!(response.error.unwrap().code, -32602);
        }

        let malformed_submit = rpc.handle(RpcRequest {
            id: Some(json!(4)),
            method: "agora_submitDrcDepositPreauth".into(),
            params: json!({ "preauth": { "version": 1 } }),
        });
        assert_eq!(malformed_submit.error.unwrap().code, -32602);

        let valid = DrcDepositPreauthTx::authorize(owner, source, Amount::from_base_units(1), 9);
        let rejected = rpc.handle(RpcRequest {
            id: Some(json!(5)),
            method: "agora_submitDrcDepositPreauth".into(),
            params: json!({ "preauth": valid }),
        });
        assert_eq!(rejected.error.unwrap().code, -32001);
    }

    #[test]
    fn drc_ticket_rpc_handles_live_unknown_malformed_and_submit_rejection() {
        let owner = Address([0x11; 20]);
        let other = Address([0x12; 20]);
        let mut backend = InMemoryBackend::new();
        backend.insert_drc_live_ticket(owner, 7);
        let mut rpc = RpcDispatcher::new(backend);

        let live = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getDrcTicket".into(),
            params: json!({
                "owner": owner.to_hex(),
                "ticket_sequence": 7,
            }),
        });
        let live_result = live.result.unwrap();
        assert_eq!(live_result["status"], "live");
        assert_eq!(live_result["ticket_sequence"], 7);
        assert_eq!(live_result["owner"], owner.to_bech32());
        assert!(live_result.get("private_key").is_none());
        assert!(live_result.as_object().unwrap().len() <= 3);

        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_getDrcTicket".into(),
            params: json!([owner.to_bech32(), 8]),
        });
        assert_eq!(unknown.result.unwrap()["status"], "unknown");

        let consumed = rpc.handle(RpcRequest {
            id: Some(json!(3)),
            method: "agora_getDrcTicket".into(),
            params: json!({
                "owner": owner.to_hex(),
                "ticket_sequence": 99,
            }),
        });
        assert_eq!(consumed.result.unwrap()["status"], "unknown");

        for params in [
            json!({}),
            json!({"owner": owner.to_hex()}),
            json!({"owner": "not-an-address", "ticket_sequence": 1}),
            json!({"owner": owner.to_hex(), "ticket_sequence": "x"}),
            json!([owner.to_hex()]),
        ] {
            let response = rpc.handle(RpcRequest {
                id: Some(json!(4)),
                method: "agora_getDrcTicket".into(),
                params,
            });
            assert_eq!(response.error.unwrap().code, -32602);
        }

        let zero_owner = rpc.handle(RpcRequest {
            id: Some(json!(5)),
            method: "agora_getDrcTicket".into(),
            params: json!({
                "owner": Address::ZERO.to_hex(),
                "ticket_sequence": 1,
            }),
        });
        assert_eq!(zero_owner.error.unwrap().code, -32602);

        let malformed_submit = rpc.handle(RpcRequest {
            id: Some(json!(6)),
            method: "agora_submitDrcTicketCreate".into(),
            params: json!({ "ticket_create": { "version": 1 } }),
        });
        assert_eq!(malformed_submit.error.unwrap().code, -32602);

        let valid = DrcTicketCreateTx::unsigned(other, Amount::from_base_units(1), 0);
        let rejected = rpc.handle(RpcRequest {
            id: Some(json!(7)),
            method: "agora_submitDrcTicketCreate".into(),
            params: json!({ "ticket_create": valid }),
        });
        assert_eq!(rejected.error.unwrap().code, -32001);
        assert!(RpcMethod::parse("agora_getDrcTicket").is_some());
        assert!(RpcMethod::parse("agora_submitDrcTicketCreate").is_some());
    }

    #[test]
    fn civic_constitution_and_text_proposal() {
        let mut rpc = RpcDispatcher::new(InMemoryBackend::new());
        let constitution = rpc.handle(RpcRequest {
            id: Some(json!(1)),
            method: "agora_getConstitution".into(),
            params: json!([]),
        });
        let c = constitution.result.unwrap();
        assert_eq!(c["id"], json!("constitution-v1"));
        assert!(c["content_hash"].as_str().unwrap().len() == 64);

        let author = Address::from_hex("aabbccddeeff00112233445566778899aabbccdd").unwrap();
        let submitted = rpc.handle(RpcRequest {
            id: Some(json!(2)),
            method: "agora_submitProposal".into(),
            params: json!({
                "author": author.to_bech32(),
                "title": "Signal",
                "summary": "hello ecclesia",
                "kind": { "type": "text_signal" },
                "slot": 1u64,
            }),
        });
        let pid = submitted.result.unwrap()["proposal_id"].as_u64().unwrap();
        assert_eq!(pid, 1);

        let min_deposit = rpc
            .handle(RpcRequest {
                id: Some(json!(3)),
                method: "agora_getGovernance".into(),
                params: json!([]),
            })
            .result
            .unwrap()["params"]["min_deposit"]
            .as_u64()
            .unwrap();
        rpc.handle(RpcRequest {
            id: Some(json!(4)),
            method: "agora_depositProposal".into(),
            params: json!({ "id": pid, "amount": min_deposit }),
        });
        rpc.handle(RpcRequest {
            id: Some(json!(5)),
            method: "agora_openProposalVoting".into(),
            params: json!({ "id": pid, "slot": 2u64 }),
        });
        let voter = Address::from_hex("11223344556677889900aabbccddeeff00112233").unwrap();
        let voted = rpc.handle(RpcRequest {
            id: Some(json!(6)),
            method: "agora_castGovVote".into(),
            params: json!({
                "id": pid,
                "voter": voter.to_bech32(),
                "choice": "yes",
                "raw_balance": 10_000u64,
                "total_supply": 10_000u64,
            }),
        });
        assert_eq!(voted.result.unwrap()["voted"], json!(true));

        let listed = rpc.handle(RpcRequest {
            id: Some(json!(7)),
            method: "agora_listProposals".into(),
            params: json!({ "limit": 8 }),
        });
        assert_eq!(listed.result.unwrap()["count"], json!(1));

        let offices = rpc.handle(RpcRequest {
            id: Some(json!(8)),
            method: "agora_listOffices".into(),
            params: json!([]),
        });
        assert!(offices.result.unwrap()["offices"].as_array().unwrap().len() >= 27);
    }

    #[test]
    fn get_drc_escrow_invalid_params_and_point_queries() {
        let mut backend = InMemoryBackend::new();
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        backend.insert_block(genesis);
        let mut rpc = RpcDispatcher::new(backend);
        let bad = rpc.handle(RpcRequest {
            id: Some(json!(90)),
            method: "agora_getDrcEscrow".into(),
            params: json!({"escrow_id": "not-a-hash"}),
        });
        assert_eq!(bad.error.as_ref().unwrap().code, -32602);
        let zero = rpc.handle(RpcRequest {
            id: Some(json!(91)),
            method: "agora_getDrcEscrow".into(),
            params: json!({"escrow_id": Hash::ZERO.to_hex()}),
        });
        assert_eq!(zero.error.as_ref().unwrap().code, -32602);
        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(92)),
            method: "agora_getDrcEscrow".into(),
            params: json!({"escrow_id": Hash([7; 32]).to_hex()}),
        });
        assert_eq!(unknown.result.unwrap()["status"], json!("unknown"));
        let receipt = rpc.handle(RpcRequest {
            id: Some(json!(93)),
            method: "agora_getDrcEscrowReceipt".into(),
            params: json!({"escrow_id": Hash([8; 32]).to_hex()}),
        });
        assert_eq!(receipt.result.unwrap()["status"], json!("unknown"));
    }

    #[test]
    fn submit_drc_escrow_malformed_structure_returns_invalid_params() {
        let mut backend = InMemoryBackend::new();
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        backend.insert_block(genesis);
        let mut rpc = RpcDispatcher::new(backend);
        let bad = rpc.handle(RpcRequest {
            id: Some(json!(94)),
            method: "agora_submitDrcEscrowCreate".into(),
            params: json!({"owner": "not-an-address"}),
        });
        assert_eq!(bad.error.as_ref().unwrap().code, -32602);
        let invoice = rpc.handle(RpcRequest {
            id: Some(json!(95)),
            method: "agora_submitDrcEscrowCreate".into(),
            params: json!({
                "version": 1,
                "owner": agora_types::Address([1;20]).to_bech32(),
                "recipient": agora_types::Address([2;20]).to_bech32(),
                "amount": "1",
                "fee": "1",
                "invoice_id": Hash([9;32]).to_hex(),
                "cancel_after_blue_score": 10,
                "nonce": 0,
                "public_key": "",
                "signature": ""
            }),
        });
        assert_eq!(invoice.error.as_ref().unwrap().code, -32602);
    }

    #[test]
    fn get_drc_check_query_malformed_and_unknown() {
        let mut backend = InMemoryBackend::new();
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        backend.insert_block(genesis);
        let mut rpc = RpcDispatcher::new(backend);
        let bad = rpc.handle(RpcRequest {
            id: Some(json!(96)),
            method: "agora_getDrcCheck".into(),
            params: json!({"check_id": "not-a-hash"}),
        });
        assert_eq!(bad.error.as_ref().unwrap().code, -32602);
        let zero = rpc.handle(RpcRequest {
            id: Some(json!(97)),
            method: "agora_getDrcCheck".into(),
            params: json!({"check_id": Hash::ZERO.to_hex()}),
        });
        assert_eq!(zero.error.as_ref().unwrap().code, -32602);
        let unknown = rpc.handle(RpcRequest {
            id: Some(json!(98)),
            method: "agora_getDrcCheck".into(),
            params: json!({"check_id": Hash([7; 32]).to_hex()}),
        });
        assert_eq!(unknown.result.unwrap()["status"], json!("unknown"));
        let receipt = rpc.handle(RpcRequest {
            id: Some(json!(99)),
            method: "agora_getDrcCheckReceipt".into(),
            params: json!({"check_id": Hash([8; 32]).to_hex()}),
        });
        assert_eq!(receipt.result.unwrap()["status"], json!("unknown"));
    }

    #[test]
    fn submit_drc_check_malformed_structure_returns_invalid_params() {
        let mut backend = InMemoryBackend::new();
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: vec![],
            account_transfers: vec![],
            stake_ops: vec![],
            ovl_executions: vec![],
            drc_payments: vec![],
            data_commitments: vec![],
            drc_account_policies: vec![],
            drc_deposit_preauths: vec![],
            drc_regular_keys: vec![],
            drc_signer_lists: vec![],
            drc_ticket_creates: vec![],
            drc_escrow_creates: vec![],
            drc_escrow_finishes: vec![],
            drc_escrow_cancels: vec![],
            drc_check_creates: vec![],
            drc_check_cashes: vec![],
            drc_check_cancels: vec![],
            drc_payment_channel_creates: vec![],
            drc_payment_channel_funds: vec![],
            drc_payment_channel_claims: vec![],
            drc_payment_channel_closes: vec![],
            drc_trust_line_sets: vec![],
            drc_issued_transfers: vec![],
            drc_multisign_attachments: vec![],
        };
        backend.insert_block(genesis);
        let mut rpc = RpcDispatcher::new(backend);
        let bad = rpc.handle(RpcRequest {
            id: Some(json!(100)),
            method: "agora_submitDrcCheckCreate".into(),
            params: json!({"owner": "not-an-address"}),
        });
        assert_eq!(bad.error.as_ref().unwrap().code, -32602);
        let bounds = rpc.handle(RpcRequest {
            id: Some(json!(101)),
            method: "agora_submitDrcCheckCreate".into(),
            params: json!({
                "version": 1,
                "owner": agora_types::Address([1;20]).to_bech32(),
                "destination": agora_types::Address([2;20]).to_bech32(),
                "amount": "1",
                "fee": "1",
                "invoice_id": Hash([9;32]).to_hex(),
                "expires_after_blue_score": 0,
                "nonce": 0,
                "public_key": "",
                "signature": ""
            }),
        });
        assert_eq!(bounds.error.as_ref().unwrap().code, -32602);
        let bad_cash = rpc.handle(RpcRequest {
            id: Some(json!(102)),
            method: "agora_submitDrcCheckCash".into(),
            params: json!({"check_id": "xy", "submitter": agora_types::Address([3;20]).to_bech32()}),
        });
        assert_eq!(bad_cash.error.as_ref().unwrap().code, -32602);
    }

    #[test]
    fn get_drc_payment_channel_query_malformed_and_unknown() {
        let backend = InMemoryBackend::new();
        let mut rpc = RpcDispatcher::new(backend);
        let bad = rpc.handle(RpcRequest {
            id: Some(json!(103)),
            method: "agora_getDrcPaymentChannel".into(),
            params: json!({"channel_id": "not-hex"}),
        });
        assert_eq!(bad.error.as_ref().unwrap().code, -32602);
        let zero = rpc.handle(RpcRequest {
            id: Some(json!(104)),
            method: "agora_getDrcPaymentChannel".into(),
            params: json!({"channel_id": Hash::ZERO.to_hex()}),
        });
        assert_eq!(zero.error.as_ref().unwrap().code, -32602);
    }
}
