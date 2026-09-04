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
