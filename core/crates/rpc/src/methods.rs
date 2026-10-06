use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::RpcError;

/// Canonical RPC method names (JSON-RPC style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethod {
    GetDagTips,
    GetBlock,
    GetTransaction,
    GetMempool,
    GetNodeInfo,
    EstimateFee,
    SubmitTransaction,
    SubmitAccountTransfer,
    SubmitOvlExecution,
    SubmitDrcPayment,
    GetDrcPayment,
    GetDrcPaymentByInvoice,
    SubmitDrcAccountPolicy,
    GetDrcAccountPolicy,
    SubmitDrcDepositPreauth,
    GetDrcDepositPreauth,
    SubmitDrcRegularKey,
    GetDrcAccountKeys,
    SubmitDrcSignerList,
    GetDrcAccountSignerList,
    SubmitDrcTicketCreate,
    GetDrcTicket,
    SubmitDrcEscrowCreate,
    SubmitDrcEscrowFinish,
    SubmitDrcEscrowCancel,
    GetDrcEscrow,
    GetDrcEscrowReceipt,
    SubmitDrcCheckCreate,
    SubmitDrcCheckCash,
    SubmitDrcCheckCancel,
    GetDrcCheck,
    GetDrcCheckReceipt,
    SubmitDrcPaymentChannelCreate,
    SubmitDrcPaymentChannelFund,
    SubmitDrcPaymentChannelClaim,
    SubmitDrcPaymentChannelClose,
    GetDrcPaymentChannel,
    GetDrcPaymentChannelReceipt,
    GetDrcPaymentChannelFundEvent,
    GetDrcPaymentChannelClaimEvent,
    GetDrcPaymentChannelScheduleEvent,
    VerifyDrcPaymentChannelClaim,
    SubmitDrcTrustLineSet,
    SubmitDrcIssuedTransfer,
    GetDrcTrustLine,
    GetDrcIssuerLiability,
    GetDrcIssuedTransferReceipt,
    SubmitDrcIssuedAssetPolicySet,
    SubmitDrcTrustLineIssuerControl,
    SubmitDrcIssuedClawback,
    GetDrcIssuedAssetPolicy,
    GetDrcIssuedAssetPolicyReceipt,
    GetDrcTrustLineIssuerControlReceipt,
    GetDrcIssuedClawbackReceipt,
    GetDrcObject,
    GetDrcAccountObjects,
    GetDrcOperation,
    GetDrcTransaction,
    GetBalance,
    GetUtxos,
    FundAddress,
    GetBlockTemplate,
    SubmitBlock,
    // Trident dual-PoS finality / staking (read + attestation submit)
    GetFinality,
    GetFinalizedTip,
    SubmitAttestation,
    GetValidatorSet,
    GetValidator,
    GetRewardPool,
    GetNativeAssetSupply,
    GetProtocolTreasuries,
    GetCommunityRegistry,
    SubmitStakeTx,
    // Civic governance + community (EOS forum / Ecclesia ballot)
    GetConstitution,
    GetGovernance,
    ListProposals,
    GetProposal,
    ListOffices,
    ListForumTopics,
    SubmitProposal,
    DepositProposal,
    OpenProposalVoting,
    CastGovVote,
    TallyProposal,
    EnterProposalTimelock,
    ExecuteProposal,
    PostForumTopic,
    AckConstitution,
    SponsorProposal,
    AssentProposal,
}

impl RpcMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GetDagTips => "agora_getDagTips",
            Self::GetBlock => "agora_getBlock",
            Self::GetTransaction => "agora_getTransaction",
            Self::GetMempool => "agora_getMempool",
            Self::GetNodeInfo => "agora_getNodeInfo",
            Self::EstimateFee => "agora_estimateFee",
            Self::SubmitTransaction => "agora_submitTransaction",
            Self::SubmitAccountTransfer => "agora_submitAccountTransfer",
            Self::SubmitOvlExecution => "agora_submitOvlExecution",
            Self::SubmitDrcPayment => "agora_submitDrcPayment",
            Self::GetDrcPayment => "agora_getDrcPayment",
            Self::GetDrcPaymentByInvoice => "agora_getDrcPaymentByInvoice",
            Self::SubmitDrcAccountPolicy => "agora_submitDrcAccountPolicy",
            Self::GetDrcAccountPolicy => "agora_getDrcAccountPolicy",
            Self::SubmitDrcDepositPreauth => "agora_submitDrcDepositPreauth",
            Self::GetDrcDepositPreauth => "agora_getDrcDepositPreauth",
            Self::SubmitDrcRegularKey => "agora_submitDrcRegularKey",
            Self::GetDrcAccountKeys => "agora_getDrcAccountKeys",
            Self::SubmitDrcSignerList => "agora_submitDrcSignerList",
            Self::GetDrcAccountSignerList => "agora_getDrcAccountSignerList",
            Self::SubmitDrcTicketCreate => "agora_submitDrcTicketCreate",
            Self::GetDrcTicket => "agora_getDrcTicket",
            Self::SubmitDrcEscrowCreate => "agora_submitDrcEscrowCreate",
            Self::SubmitDrcEscrowFinish => "agora_submitDrcEscrowFinish",
            Self::SubmitDrcEscrowCancel => "agora_submitDrcEscrowCancel",
            Self::GetDrcEscrow => "agora_getDrcEscrow",
            Self::GetDrcEscrowReceipt => "agora_getDrcEscrowReceipt",
            Self::SubmitDrcCheckCreate => "agora_submitDrcCheckCreate",
            Self::SubmitDrcCheckCash => "agora_submitDrcCheckCash",
            Self::SubmitDrcCheckCancel => "agora_submitDrcCheckCancel",
            Self::GetDrcCheck => "agora_getDrcCheck",
            Self::GetDrcCheckReceipt => "agora_getDrcCheckReceipt",
            Self::SubmitDrcPaymentChannelCreate => "agora_submitDrcPaymentChannelCreate",
            Self::SubmitDrcPaymentChannelFund => "agora_submitDrcPaymentChannelFund",
            Self::SubmitDrcPaymentChannelClaim => "agora_submitDrcPaymentChannelClaim",
            Self::SubmitDrcPaymentChannelClose => "agora_submitDrcPaymentChannelClose",
            Self::GetDrcPaymentChannel => "agora_getDrcPaymentChannel",
            Self::GetDrcPaymentChannelReceipt => "agora_getDrcPaymentChannelReceipt",
            Self::GetDrcPaymentChannelFundEvent => "agora_getDrcPaymentChannelFundEvent",
            Self::GetDrcPaymentChannelClaimEvent => "agora_getDrcPaymentChannelClaimEvent",
            Self::GetDrcPaymentChannelScheduleEvent => "agora_getDrcPaymentChannelScheduleEvent",
            Self::VerifyDrcPaymentChannelClaim => "agora_verifyDrcPaymentChannelClaim",
            Self::SubmitDrcTrustLineSet => "agora_submitDrcTrustLineSet",
            Self::SubmitDrcIssuedTransfer => "agora_submitDrcIssuedTransfer",
            Self::GetDrcTrustLine => "agora_getDrcTrustLine",
            Self::GetDrcIssuerLiability => "agora_getDrcIssuerLiability",
            Self::GetDrcIssuedTransferReceipt => "agora_getDrcIssuedTransferReceipt",
            Self::SubmitDrcIssuedAssetPolicySet => "agora_submitDrcIssuedAssetPolicySet",
            Self::SubmitDrcTrustLineIssuerControl => "agora_submitDrcTrustLineIssuerControl",
            Self::SubmitDrcIssuedClawback => "agora_submitDrcIssuedClawback",
            Self::GetDrcIssuedAssetPolicy => "agora_getDrcIssuedAssetPolicy",
            Self::GetDrcIssuedAssetPolicyReceipt => "agora_getDrcIssuedAssetPolicyReceipt",
            Self::GetDrcTrustLineIssuerControlReceipt => {
                "agora_getDrcTrustLineIssuerControlReceipt"
            }
            Self::GetDrcIssuedClawbackReceipt => "agora_getDrcIssuedClawbackReceipt",
            Self::GetDrcObject => "agora_getDrcObject",
            Self::GetDrcAccountObjects => "agora_getDrcAccountObjects",
            Self::GetDrcOperation => "agora_getDrcOperation",
            Self::GetDrcTransaction => "agora_getDrcTransaction",
            Self::GetBalance => "agora_getBalance",
            Self::GetUtxos => "agora_getUtxos",
            Self::FundAddress => "agora_fundAddress",
            Self::GetBlockTemplate => "agora_getBlockTemplate",
            Self::SubmitBlock => "agora_submitBlock",
            Self::GetFinality => "agora_getFinality",
            Self::GetFinalizedTip => "agora_getFinalizedTip",
            Self::SubmitAttestation => "agora_submitAttestation",
            Self::GetValidatorSet => "agora_getValidatorSet",
            Self::GetValidator => "agora_getValidator",
            Self::GetRewardPool => "agora_getRewardPool",
            Self::GetNativeAssetSupply => "agora_getNativeAssetSupply",
            Self::GetProtocolTreasuries => "agora_getProtocolTreasuries",
            Self::GetCommunityRegistry => "agora_getCommunityRegistry",
            Self::SubmitStakeTx => "agora_submitStakeTx",
            Self::GetConstitution => "agora_getConstitution",
            Self::GetGovernance => "agora_getGovernance",
            Self::ListProposals => "agora_listProposals",
            Self::GetProposal => "agora_getProposal",
            Self::ListOffices => "agora_listOffices",
            Self::ListForumTopics => "agora_listForumTopics",
            Self::SubmitProposal => "agora_submitProposal",
            Self::DepositProposal => "agora_depositProposal",
            Self::OpenProposalVoting => "agora_openProposalVoting",
            Self::CastGovVote => "agora_castGovVote",
            Self::TallyProposal => "agora_tallyProposal",
            Self::EnterProposalTimelock => "agora_enterProposalTimelock",
            Self::ExecuteProposal => "agora_executeProposal",
            Self::PostForumTopic => "agora_postForumTopic",
            Self::AckConstitution => "agora_ackConstitution",
            Self::SponsorProposal => "agora_sponsorProposal",
            Self::AssentProposal => "agora_assentProposal",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "agora_getDagTips" => Some(Self::GetDagTips),
            "agora_getBlock" => Some(Self::GetBlock),
            "agora_getTransaction" => Some(Self::GetTransaction),
            "agora_getMempool" => Some(Self::GetMempool),
            "agora_getNodeInfo" => Some(Self::GetNodeInfo),
            "agora_estimateFee" => Some(Self::EstimateFee),
            "agora_submitTransaction" => Some(Self::SubmitTransaction),
            "agora_submitAccountTransfer" => Some(Self::SubmitAccountTransfer),
            "agora_submitOvlExecution" => Some(Self::SubmitOvlExecution),
            "agora_submitDrcPayment" => Some(Self::SubmitDrcPayment),
            "agora_getDrcPayment" => Some(Self::GetDrcPayment),
            "agora_getDrcPaymentByInvoice" => Some(Self::GetDrcPaymentByInvoice),
            "agora_submitDrcAccountPolicy" => Some(Self::SubmitDrcAccountPolicy),
            "agora_getDrcAccountPolicy" => Some(Self::GetDrcAccountPolicy),
            "agora_submitDrcDepositPreauth" => Some(Self::SubmitDrcDepositPreauth),
            "agora_getDrcDepositPreauth" => Some(Self::GetDrcDepositPreauth),
            "agora_submitDrcRegularKey" => Some(Self::SubmitDrcRegularKey),
            "agora_getDrcAccountKeys" => Some(Self::GetDrcAccountKeys),
            "agora_submitDrcSignerList" => Some(Self::SubmitDrcSignerList),
            "agora_getDrcAccountSignerList" => Some(Self::GetDrcAccountSignerList),
            "agora_submitDrcTicketCreate" => Some(Self::SubmitDrcTicketCreate),
            "agora_getDrcTicket" => Some(Self::GetDrcTicket),
            "agora_submitDrcEscrowCreate" => Some(Self::SubmitDrcEscrowCreate),
            "agora_submitDrcEscrowFinish" => Some(Self::SubmitDrcEscrowFinish),
            "agora_submitDrcEscrowCancel" => Some(Self::SubmitDrcEscrowCancel),
            "agora_getDrcEscrow" => Some(Self::GetDrcEscrow),
            "agora_getDrcEscrowReceipt" => Some(Self::GetDrcEscrowReceipt),
            "agora_submitDrcCheckCreate" => Some(Self::SubmitDrcCheckCreate),
            "agora_submitDrcCheckCash" => Some(Self::SubmitDrcCheckCash),
            "agora_submitDrcCheckCancel" => Some(Self::SubmitDrcCheckCancel),
            "agora_getDrcCheck" => Some(Self::GetDrcCheck),
            "agora_getDrcCheckReceipt" => Some(Self::GetDrcCheckReceipt),
            "agora_submitDrcPaymentChannelCreate" => Some(Self::SubmitDrcPaymentChannelCreate),
            "agora_submitDrcPaymentChannelFund" => Some(Self::SubmitDrcPaymentChannelFund),
            "agora_submitDrcPaymentChannelClaim" => Some(Self::SubmitDrcPaymentChannelClaim),
            "agora_submitDrcPaymentChannelClose" => Some(Self::SubmitDrcPaymentChannelClose),
            "agora_getDrcPaymentChannel" => Some(Self::GetDrcPaymentChannel),
            "agora_getDrcPaymentChannelReceipt" => Some(Self::GetDrcPaymentChannelReceipt),
            "agora_getDrcPaymentChannelFundEvent" => Some(Self::GetDrcPaymentChannelFundEvent),
            "agora_getDrcPaymentChannelClaimEvent" => Some(Self::GetDrcPaymentChannelClaimEvent),
            "agora_getDrcPaymentChannelScheduleEvent" => {
                Some(Self::GetDrcPaymentChannelScheduleEvent)
            }
            "agora_verifyDrcPaymentChannelClaim" => Some(Self::VerifyDrcPaymentChannelClaim),
            "agora_submitDrcTrustLineSet" => Some(Self::SubmitDrcTrustLineSet),
            "agora_submitDrcIssuedTransfer" => Some(Self::SubmitDrcIssuedTransfer),
            "agora_getDrcTrustLine" => Some(Self::GetDrcTrustLine),
            "agora_getDrcIssuerLiability" => Some(Self::GetDrcIssuerLiability),
            "agora_getDrcIssuedTransferReceipt" => Some(Self::GetDrcIssuedTransferReceipt),
            "agora_submitDrcIssuedAssetPolicySet" => Some(Self::SubmitDrcIssuedAssetPolicySet),
            "agora_submitDrcTrustLineIssuerControl" => Some(Self::SubmitDrcTrustLineIssuerControl),
            "agora_submitDrcIssuedClawback" => Some(Self::SubmitDrcIssuedClawback),
            "agora_getDrcIssuedAssetPolicy" => Some(Self::GetDrcIssuedAssetPolicy),
            "agora_getDrcIssuedAssetPolicyReceipt" => Some(Self::GetDrcIssuedAssetPolicyReceipt),
            "agora_getDrcTrustLineIssuerControlReceipt" => {
                Some(Self::GetDrcTrustLineIssuerControlReceipt)
            }
            "agora_getDrcIssuedClawbackReceipt" => Some(Self::GetDrcIssuedClawbackReceipt),
            "agora_getDrcObject" => Some(Self::GetDrcObject),
            "agora_getDrcAccountObjects" => Some(Self::GetDrcAccountObjects),
            "agora_getDrcOperation" => Some(Self::GetDrcOperation),
            "agora_getDrcTransaction" => Some(Self::GetDrcTransaction),
            "agora_getBalance" => Some(Self::GetBalance),
            "agora_getUtxos" => Some(Self::GetUtxos),
            "agora_fundAddress" => Some(Self::FundAddress),
            "agora_getBlockTemplate" => Some(Self::GetBlockTemplate),
            "agora_submitBlock" => Some(Self::SubmitBlock),
            "agora_getFinality" => Some(Self::GetFinality),
            "agora_getFinalizedTip" => Some(Self::GetFinalizedTip),
            "agora_submitAttestation" => Some(Self::SubmitAttestation),
            "agora_getValidatorSet" => Some(Self::GetValidatorSet),
            "agora_getValidator" => Some(Self::GetValidator),
            "agora_getRewardPool" => Some(Self::GetRewardPool),
            "agora_getNativeAssetSupply" => Some(Self::GetNativeAssetSupply),
            "agora_getProtocolTreasuries" => Some(Self::GetProtocolTreasuries),
            "agora_getCommunityRegistry" => Some(Self::GetCommunityRegistry),
            "agora_submitStakeTx" => Some(Self::SubmitStakeTx),
            "agora_getConstitution" => Some(Self::GetConstitution),
            "agora_getGovernance" => Some(Self::GetGovernance),
            "agora_listProposals" => Some(Self::ListProposals),
            "agora_getProposal" => Some(Self::GetProposal),
            "agora_listOffices" => Some(Self::ListOffices),
            "agora_listForumTopics" => Some(Self::ListForumTopics),
            "agora_submitProposal" => Some(Self::SubmitProposal),
            "agora_depositProposal" => Some(Self::DepositProposal),
            "agora_openProposalVoting" => Some(Self::OpenProposalVoting),
            "agora_castGovVote" => Some(Self::CastGovVote),
            "agora_tallyProposal" => Some(Self::TallyProposal),
            "agora_enterProposalTimelock" => Some(Self::EnterProposalTimelock),
            "agora_executeProposal" => Some(Self::ExecuteProposal),
            "agora_postForumTopic" => Some(Self::PostForumTopic),
            "agora_ackConstitution" => Some(Self::AckConstitution),
            "agora_sponsorProposal" => Some(Self::SponsorProposal),
            "agora_assentProposal" => Some(Self::AssentProposal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorBody {
    pub code: i64,
    pub message: String,
}

impl RpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Value>, err: &RpcError) -> Self {
        Self {
            id,
            result: None,
            error: Some(RpcErrorBody {
                code: err.code(),
                message: err.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RpcMethod;

    #[test]
    fn programmable_execution_rpc_is_ovl_only() {
        assert_eq!(
            RpcMethod::parse("agora_submitOvlExecution"),
            Some(RpcMethod::SubmitOvlExecution)
        );

        for forbidden in [
            "agora_submitExecution",
            "agora_submitDrcExecution",
            "agora_deployDrcContract",
            "agora_callDrcContract",
            "agora_submitDrcVmTransaction",
        ] {
            assert_eq!(RpcMethod::parse(forbidden), None, "{forbidden}");
        }
    }
}
