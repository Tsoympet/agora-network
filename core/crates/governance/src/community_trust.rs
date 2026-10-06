//! Local community attributes, proposal transparency, reward funding limits,
//! public aggregates, and device-local identity export.
//!
//! Maturity: Scaffold. This module does not admit blocks, mint assets, or
//! read a device location. TLT issuance stays in the PoW monetary policy.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::proposal::{Proposal, ProposalStatus};

pub const MANUAL_REGION_SEARCH_NOTICE: &str = "Type a country, region, city, language, interest, or specialization. Search uses that text on this device. GPS is not requested.";

pub const PORTABLE_IDENTITY_FORMAT: &str = "agora-portable-identity-v1";
pub const PUBLIC_PASSPORT_VERSION: &str = "agora-public-passport-v1";
pub const LOCAL_PREFS_VERSION: &str = "agora-local-prefs-v1";
pub const PASSPORT_EXPORT_NOTE: &str = "Public contribution evidence. This export is not a personhood check and it is not Sybil resistance.";

const TLT_EMISSION_BLOCK: &str = concat!(
    "TLT community rewards are not payable from PoW emission. ",
    "The emission schedule is unchanged. This offer is blocked in the UI."
);

const COMMUNITY_FORBIDDEN: &[&str] = &[
    "latitude",
    "longitude",
    "lat",
    "lon",
    "lng",
    "gps",
    "coordinates",
    "geolocation",
    "altitude",
];

const PRIVATE_ANALYTICS_KEYS: &[&str] = &[
    "user_id",
    "userId",
    "address",
    "wallet",
    "ip",
    "email",
    "device_id",
    "deviceId",
    "mnemonic",
    "private_key",
    "privateKey",
    "session",
    "gps",
    "latitude",
    "longitude",
    "name",
    "account",
];

const PUBLIC_ANALYTICS_KEYS: &[&str] = &[
    "active_users",
    "active_developers",
    "active_merchants",
    "missions",
    "grants",
    "bounties",
    "academy",
    "events",
    "governance",
    "drc_activity",
    "ovl_contracts",
    "tlt_activity",
    "nodes",
    "miners",
    "projects",
];

const SECRET_JSON_KEYS: &[&str] = &[
    "mnemonic",
    "private_key",
    "privateKey",
    "seed",
    "xprv",
    "secret",
    "vault_password",
    "wallet_password",
    "privkey",
];

const TRUST_IDS: [&str; 10] = [
    "headers",
    "merkle",
    "state_proofs",
    "rpc",
    "indexer",
    "community_api",
    "wallet",
    "passport",
    "governance",
    "confirmation",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommunityTrustError {
    #[error("local community record is invalid: {0}")]
    InvalidCommunity(String),
    #[error("proposal transparency is invalid: {0}")]
    InvalidTransparency(String),
    #[error("public analytics rejected: {0}")]
    PrivateAnalytics(String),
    #[error("portable identity export rejected: {0}")]
    IdentityExport(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCommunity {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interests: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specializations: Vec<String>,
}

/// Typed attribute search. Callers pass text. There is no coordinate argument.
pub fn community_search_uses_device_location() -> bool {
    false
}

pub fn seed_local_communities() -> Vec<LocalCommunity> {
    [
        LocalCommunity {
            id: "greece-attica-athens".into(),
            name: "Athens Agora".into(),
            country: Some("Greece".into()),
            region: Some("Attica".into()),
            city: Some("Athens".into()),
            languages: vec!["el".into(), "en".into()],
            interests: vec!["governance".into(), "civic assembly".into()],
            specializations: vec!["constitution".into(), "public square".into()],
        },
        LocalCommunity {
            id: "europe".into(),
            name: "Europe".into(),
            country: None,
            region: Some("Europe".into()),
            city: None,
            languages: vec!["en".into(), "el".into(), "de".into(), "fr".into()],
            interests: vec!["payments".into(), "merchants".into()],
            specializations: vec!["DRC payments".into()],
        },
        LocalCommunity {
            id: "asia".into(),
            name: "Asia".into(),
            country: None,
            region: Some("Asia".into()),
            city: None,
            languages: vec!["en".into(), "zh".into(), "ja".into(), "ko".into()],
            interests: vec!["developers".into(), "academy".into()],
            specializations: vec!["protocol engineering".into()],
        },
        LocalCommunity {
            id: "americas".into(),
            name: "Americas".into(),
            country: None,
            region: Some("Americas".into()),
            city: None,
            languages: vec!["en".into(), "es".into(), "pt".into()],
            interests: vec!["miners".into(), "nodes".into()],
            specializations: vec!["TLT infrastructure".into()],
        },
    ]
    .into_iter()
    .map(|community| normalize_local_community(community).expect("seed local community"))
    .collect()
}

pub fn local_community_from_json(bytes: &str) -> Result<LocalCommunity, CommunityTrustError> {
    let value: Value = serde_json::from_str(bytes)
        .map_err(|err| CommunityTrustError::InvalidCommunity(err.to_string()))?;
    if let Some(obj) = value.as_object() {
        for key in obj.keys() {
            if COMMUNITY_FORBIDDEN.iter().any(|forbidden| forbidden == key) {
                return Err(CommunityTrustError::InvalidCommunity(format!(
                    "field {key} is not a community attribute"
                )));
            }
        }
    }
    let community: LocalCommunity = serde_json::from_value(value)
        .map_err(|err| CommunityTrustError::InvalidCommunity(err.to_string()))?;
    normalize_local_community(community)
}

pub fn search_local_communities<'a>(
    records: &'a [LocalCommunity],
    query: &str,
) -> Vec<&'a LocalCommunity> {
    let trimmed = query.trim().to_lowercase();
    if trimmed.is_empty() {
        return records.iter().collect();
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    records
        .iter()
        .filter(|record| {
            let hay = search_haystack(record);
            tokens.iter().all(|token| hay.contains(token))
        })
        .collect()
}

fn search_haystack(record: &LocalCommunity) -> String {
    let mut parts = vec![record.id.clone(), record.name.clone()];
    parts.extend(
        [
            record.country.clone(),
            record.region.clone(),
            record.city.clone(),
        ]
        .into_iter()
        .flatten(),
    );
    parts.extend(record.languages.iter().cloned());
    parts.extend(record.interests.iter().cloned());
    parts.extend(record.specializations.iter().cloned());
    parts.join(" ").to_lowercase()
}

fn normalize_local_community(
    community: LocalCommunity,
) -> Result<LocalCommunity, CommunityTrustError> {
    let id = community.id.trim();
    if !valid_id(id) {
        return Err(CommunityTrustError::InvalidCommunity(
            "id must be a lowercase slug".into(),
        ));
    }
    let name = required_label(&community.name, 80, "name")?;
    let country = optional_label(community.country, 80, "country")?;
    let region = optional_label(community.region, 80, "region")?;
    let city = optional_label(community.city, 80, "city")?;
    let languages = label_list(community.languages, "language")?;
    let interests = label_list(community.interests, "interest")?;
    let specializations = label_list(community.specializations, "specialization")?;
    if country.is_none()
        && region.is_none()
        && city.is_none()
        && languages.is_empty()
        && interests.is_empty()
        && specializations.is_empty()
    {
        return Err(CommunityTrustError::InvalidCommunity(
            "a community needs a country, region, city, language, interest, or specialization"
                .into(),
        ));
    }
    Ok(LocalCommunity {
        id: id.to_string(),
        name,
        country,
        region,
        city,
        languages,
        interests,
        specializations,
    })
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let first = bytes[0];
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn required_label(value: &str, max: usize, field: &str) -> Result<String, CommunityTrustError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max || trimmed.chars().any(char::is_control)
    {
        return Err(CommunityTrustError::InvalidCommunity(format!(
            "{field} length"
        )));
    }
    Ok(trimmed.to_string())
}

fn optional_label(
    value: Option<String>,
    max: usize,
    field: &str,
) -> Result<Option<String>, CommunityTrustError> {
    match value {
        None => Ok(None),
        Some(raw) => Ok(Some(required_label(&raw, max, field)?)),
    }
}

fn label_list(values: Vec<String>, field: &str) -> Result<Vec<String>, CommunityTrustError> {
    if values.len() > 16 {
        return Err(CommunityTrustError::InvalidCommunity(format!(
            "too many {field} values"
        )));
    }
    values
        .into_iter()
        .map(|value| required_label(&value, 64, field))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalAuthority {
    OnChain,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalFinalResult {
    Pending,
    Passed,
    Failed,
    Expired,
    AdvisoryRecorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationStatus {
    NotStarted,
    InProgress,
    Shipped,
    WontImplement,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VotingPeriod {
    pub start_slot: u64,
    pub end_slot: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VoteSnapshot {
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
    pub no_with_veto: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalTransparency {
    pub id: String,
    pub authority: ProposalAuthority,
    pub creator: String,
    pub why: String,
    pub what_changes: String,
    pub expected_cost: String,
    pub treasury_impact: String,
    pub voting_period: VotingPeriod,
    pub eligibility: String,
    pub current_votes: VoteSnapshot,
    pub final_result: ProposalFinalResult,
    pub implementation_status: ImplementationStatus,
    pub commit_links: Vec<String>,
    pub release_links: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalTransparencyView {
    #[serde(flatten)]
    pub proposal: ProposalTransparency,
    pub badge: &'static str,
}

pub fn authority_badge(authority: ProposalAuthority) -> &'static str {
    match authority {
        ProposalAuthority::OnChain => "ON-CHAIN",
        ProposalAuthority::Advisory => "ADVISORY",
    }
}

pub fn proposal_view(proposal: ProposalTransparency) -> ProposalTransparencyView {
    let badge = authority_badge(proposal.authority);
    ProposalTransparencyView { proposal, badge }
}

#[derive(Debug, Clone)]
pub struct TransparencyNarrative {
    pub why: String,
    pub what_changes: String,
    pub expected_cost: String,
    pub treasury_impact: String,
    pub eligibility: String,
    pub voting_start_slot: Option<u64>,
    pub voting_end_slot: Option<u64>,
    pub implementation_status: ImplementationStatus,
    pub commit_links: Vec<String>,
    pub release_links: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AdvisoryDraft {
    pub id: String,
    pub creator: String,
    pub why: String,
    pub what_changes: String,
    pub expected_cost: String,
    pub treasury_impact: String,
    pub voting_start_slot: u64,
    pub voting_end_slot: u64,
    pub eligibility: String,
    pub current_votes: VoteSnapshot,
    pub final_result: ProposalFinalResult,
    pub implementation_status: ImplementationStatus,
    pub commit_links: Vec<String>,
    pub release_links: Vec<String>,
}

pub fn transparency_from_proposal(
    proposal: &Proposal,
    narrative: TransparencyNarrative,
) -> Result<ProposalTransparency, CommunityTrustError> {
    if matches!(proposal.status, ProposalStatus::Executed)
        && matches!(
            narrative.implementation_status,
            ImplementationStatus::NotStarted | ImplementationStatus::NotApplicable
        )
    {
        return Err(CommunityTrustError::InvalidTransparency(
            "an executed proposal needs an implementation status".into(),
        ));
    }
    let (start_slot, end_slot) = match (proposal.voting_start_slot, proposal.voting_end_slot) {
        (Some(start), Some(end)) => (start, end),
        _ => match (narrative.voting_start_slot, narrative.voting_end_slot) {
            (Some(start), Some(end)) => (start, end),
            _ => {
                return Err(CommunityTrustError::InvalidTransparency(
                    "voting period is required".into(),
                ))
            }
        },
    };
    let record = ProposalTransparency {
        id: proposal.id.to_string(),
        authority: ProposalAuthority::OnChain,
        creator: hex::encode(proposal.author.0),
        why: narrative.why,
        what_changes: narrative.what_changes,
        expected_cost: narrative.expected_cost,
        treasury_impact: narrative.treasury_impact,
        voting_period: VotingPeriod {
            start_slot,
            end_slot,
        },
        eligibility: narrative.eligibility,
        current_votes: VoteSnapshot {
            yes: proposal.tally.yes,
            no: proposal.tally.no,
            abstain: proposal.tally.abstain,
            no_with_veto: proposal.tally.no_with_veto,
        },
        final_result: final_result_for(proposal.status),
        implementation_status: narrative.implementation_status,
        commit_links: narrative.commit_links,
        release_links: narrative.release_links,
    };
    validate_transparency(&record)?;
    Ok(record)
}

pub fn advisory_transparency(
    draft: AdvisoryDraft,
) -> Result<ProposalTransparency, CommunityTrustError> {
    let record = ProposalTransparency {
        id: draft.id,
        authority: ProposalAuthority::Advisory,
        creator: draft.creator,
        why: draft.why,
        what_changes: draft.what_changes,
        expected_cost: draft.expected_cost,
        treasury_impact: draft.treasury_impact,
        voting_period: VotingPeriod {
            start_slot: draft.voting_start_slot,
            end_slot: draft.voting_end_slot,
        },
        eligibility: draft.eligibility,
        current_votes: draft.current_votes,
        final_result: draft.final_result,
        implementation_status: draft.implementation_status,
        commit_links: draft.commit_links,
        release_links: draft.release_links,
    };
    validate_transparency(&record)?;
    Ok(record)
}

fn final_result_for(status: ProposalStatus) -> ProposalFinalResult {
    match status {
        ProposalStatus::Draft | ProposalStatus::Deposit | ProposalStatus::Voting => {
            ProposalFinalResult::Pending
        }
        ProposalStatus::Passed | ProposalStatus::Timelock | ProposalStatus::Executed => {
            ProposalFinalResult::Passed
        }
        ProposalStatus::Rejected | ProposalStatus::FailedQuorum | ProposalStatus::Vetoed => {
            ProposalFinalResult::Failed
        }
        ProposalStatus::Expired => ProposalFinalResult::Expired,
    }
}

fn validate_transparency(record: &ProposalTransparency) -> Result<(), CommunityTrustError> {
    let id = record.id.trim();
    let numeric = !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit()) && id.len() <= 20;
    if !numeric && !valid_id(id) {
        return Err(CommunityTrustError::InvalidTransparency(
            "proposal id is invalid".into(),
        ));
    }
    required_transparency(&record.creator, 128, "creator")?;
    required_transparency(&record.why, 4_000, "why")?;
    required_transparency(&record.what_changes, 4_000, "what changes")?;
    required_transparency(&record.expected_cost, 4_000, "expected cost")?;
    required_transparency(&record.treasury_impact, 4_000, "treasury impact")?;
    required_transparency(&record.eligibility, 4_000, "eligibility")?;
    if record.voting_period.end_slot <= record.voting_period.start_slot {
        return Err(CommunityTrustError::InvalidTransparency(
            "voting period must end after it starts".into(),
        ));
    }
    validate_links(&record.commit_links)?;
    validate_links(&record.release_links)?;
    if record.implementation_status == ImplementationStatus::Shipped
        && record.commit_links.is_empty()
        && record.release_links.is_empty()
    {
        return Err(CommunityTrustError::InvalidTransparency(
            "shipped status needs a commit or release link".into(),
        ));
    }
    match record.authority {
        ProposalAuthority::Advisory => {
            if !matches!(
                record.final_result,
                ProposalFinalResult::Pending | ProposalFinalResult::AdvisoryRecorded
            ) {
                return Err(CommunityTrustError::InvalidTransparency(
                    "advisory proposals stay pending or advisory_recorded".into(),
                ));
            }
        }
        ProposalAuthority::OnChain => {
            if record.final_result == ProposalFinalResult::AdvisoryRecorded {
                return Err(CommunityTrustError::InvalidTransparency(
                    "on-chain proposals do not use advisory_recorded".into(),
                ));
            }
            if record.implementation_status == ImplementationStatus::NotApplicable {
                return Err(CommunityTrustError::InvalidTransparency(
                    "on-chain proposals keep an implementation status".into(),
                ));
            }
        }
    }
    Ok(())
}

fn required_transparency(value: &str, max: usize, field: &str) -> Result<(), CommunityTrustError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max || trimmed.chars().any(char::is_control)
    {
        return Err(CommunityTrustError::InvalidTransparency(format!(
            "{field} length"
        )));
    }
    Ok(())
}

fn validate_links(links: &[String]) -> Result<(), CommunityTrustError> {
    if links.len() > 8 {
        return Err(CommunityTrustError::InvalidTransparency(
            "too many links".into(),
        ));
    }
    for link in links {
        if !valid_public_link(link) {
            return Err(CommunityTrustError::InvalidTransparency(format!(
                "link {link} is not an http(s) url"
            )));
        }
    }
    Ok(())
}

fn valid_public_link(link: &str) -> bool {
    if link.len() > 200
        || link.chars().any(char::is_whitespace)
        || link.contains('@')
        || link.contains('\\')
    {
        return false;
    }
    if let Some(rest) = link.strip_prefix("https://") {
        return host_of(rest).is_some();
    }
    if let Some(rest) = link.strip_prefix("http://") {
        let Some(host) = host_of(rest) else {
            return false;
        };
        let bare = host.split(':').next().unwrap_or("");
        return bare == "localhost" || bare == "127.0.0.1";
    }
    false
}

fn host_of(rest: &str) -> Option<&str> {
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardKind {
    Drc,
    Ovl,
    Tlt,
    Badge,
    Reputation,
    Certificate,
    GrantEligibility,
    ProgramAccess,
    EventCredential,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RewardSource {
    Emission,
    ExistingTreasury {
        available_base_units: u64,
        amount_base_units: u64,
    },
    Attestation,
    ProgramRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardConfig {
    pub kind: RewardKind,
    pub label: String,
    pub source: RewardSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardDecision {
    pub kind: RewardKind,
    pub label: String,
    pub payable: bool,
    pub ui_blocked: bool,
    pub changes_tlt_emission: bool,
    pub supply_delta_base_units: u64,
    pub funding_note: String,
    pub block_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardControl {
    pub disabled: bool,
    pub label: &'static str,
    pub submits_transaction: bool,
}

pub fn default_reward_program() -> Vec<RewardConfig> {
    vec![
        RewardConfig {
            kind: RewardKind::Drc,
            label: "DRC".into(),
            source: RewardSource::ExistingTreasury {
                available_base_units: 0,
                amount_base_units: 1,
            },
        },
        RewardConfig {
            kind: RewardKind::Ovl,
            label: "OVL".into(),
            source: RewardSource::ExistingTreasury {
                available_base_units: 0,
                amount_base_units: 1,
            },
        },
        RewardConfig {
            kind: RewardKind::Tlt,
            label: "TLT from emission".into(),
            source: RewardSource::Emission,
        },
        RewardConfig {
            kind: RewardKind::Tlt,
            label: "TLT from existing treasury".into(),
            source: RewardSource::ExistingTreasury {
                available_base_units: 0,
                amount_base_units: 1,
            },
        },
        RewardConfig {
            kind: RewardKind::Badge,
            label: "Badge".into(),
            source: RewardSource::Attestation,
        },
        RewardConfig {
            kind: RewardKind::Reputation,
            label: "Reputation".into(),
            source: RewardSource::Attestation,
        },
        RewardConfig {
            kind: RewardKind::Certificate,
            label: "Certificate".into(),
            source: RewardSource::Attestation,
        },
        RewardConfig {
            kind: RewardKind::GrantEligibility,
            label: "Grant eligibility".into(),
            source: RewardSource::ProgramRule,
        },
        RewardConfig {
            kind: RewardKind::ProgramAccess,
            label: "Program access".into(),
            source: RewardSource::ProgramRule,
        },
        RewardConfig {
            kind: RewardKind::EventCredential,
            label: "Event credential".into(),
            source: RewardSource::Attestation,
        },
    ]
}

pub fn evaluate_reward(config: &RewardConfig) -> RewardDecision {
    if config.label.trim().is_empty() {
        return decision(
            config,
            false,
            String::new(),
            Some("reward label is empty".into()),
        );
    }
    let monetary = matches!(
        config.kind,
        RewardKind::Drc | RewardKind::Ovl | RewardKind::Tlt
    );
    match &config.source {
        RewardSource::Emission => {
            if config.kind == RewardKind::Tlt {
                decision(
                    config,
                    false,
                    TLT_EMISSION_BLOCK.into(),
                    Some(TLT_EMISSION_BLOCK.into()),
                )
            } else if monetary {
                decision(
                    config,
                    false,
                    String::new(),
                    Some(
                        "DRC and OVL rewards are not payable from a new mint. Community programs do not inflate those assets."
                            .into(),
                    ),
                )
            } else {
                credential_funding_blocked(config)
            }
        }
        RewardSource::ExistingTreasury { .. } if !monetary => credential_funding_blocked(config),
        RewardSource::Attestation | RewardSource::ProgramRule if monetary => decision(
            config,
            false,
            String::new(),
            Some(
                "DRC, OVL, and TLT rewards need an existing treasury balance of already-issued coins. An attestation does not mint."
                    .into(),
            ),
        ),
        RewardSource::Attestation | RewardSource::ProgramRule => decision(
            config,
            true,
            "Credential only. No native asset moves and no supply changes.".into(),
            None,
        ),
        RewardSource::ExistingTreasury {
            available_base_units,
            amount_base_units,
        } => fund_from_treasury(config, *available_base_units, *amount_base_units),
    }
}

fn credential_funding_blocked(config: &RewardConfig) -> RewardDecision {
    decision(
        config,
        false,
        String::new(),
        Some(
            "Badges, reputation, certificates, grant eligibility, program access, and event credentials are credentials. Emission and treasury spends do not fund them."
                .into(),
        ),
    )
}

fn fund_from_treasury(
    config: &RewardConfig,
    available_base_units: u64,
    amount_base_units: u64,
) -> RewardDecision {
    if amount_base_units == 0 {
        return decision(config, false, String::new(), Some("amount is zero".into()));
    }
    if available_base_units < amount_base_units {
        return decision(
            config,
            false,
            String::new(),
            Some(
                "already-issued balance does not cover the reward. Emission is not used to fill the gap."
                    .into(),
            ),
        );
    }
    let note = match config.kind {
        RewardKind::Tlt => {
            "Transfer of already-issued TLT. The PoW emission schedule is unchanged. This decision does not mint."
        }
        RewardKind::Drc => "Transfer of already-issued DRC. This decision does not mint.",
        RewardKind::Ovl => "Transfer of already-issued OVL. This decision does not mint.",
        _ => "Transfer of already-issued coins. This decision does not mint.",
    };
    decision(config, true, note.into(), None)
}

fn decision(
    config: &RewardConfig,
    payable: bool,
    funding_note: String,
    block_reason: Option<String>,
) -> RewardDecision {
    RewardDecision {
        kind: config.kind,
        label: config.label.clone(),
        payable,
        ui_blocked: !payable,
        changes_tlt_emission: false,
        supply_delta_base_units: 0,
        funding_note,
        block_reason,
    }
}

pub fn reward_control(decision: &RewardDecision) -> RewardControl {
    let blocked = decision.ui_blocked || !decision.payable;
    RewardControl {
        disabled: blocked,
        label: if blocked {
            "Blocked"
        } else {
            "Show funding path"
        },
        submits_transaction: false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicAnalyticsAggregates {
    pub active_users: u64,
    pub active_developers: u64,
    pub active_merchants: u64,
    pub missions: u64,
    pub grants: u64,
    pub bounties: u64,
    pub academy: u64,
    pub events: u64,
    pub governance: u64,
    pub drc_activity: u64,
    pub ovl_contracts: u64,
    pub tlt_activity: u64,
    pub nodes: u64,
    pub miners: u64,
    pub projects: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicAnalyticsReport {
    pub maturity: &'static str,
    pub private_user_analytics: bool,
    pub aggregates: PublicAnalyticsAggregates,
    pub assumption: &'static str,
}

pub fn scaffold_public_analytics() -> PublicAnalyticsReport {
    PublicAnalyticsReport {
        maturity: "scaffold",
        private_user_analytics: false,
        aggregates: PublicAnalyticsAggregates {
            active_users: 0,
            active_developers: 0,
            active_merchants: 0,
            missions: 0,
            grants: 0,
            bounties: 0,
            academy: 0,
            events: 0,
            governance: 0,
            drc_activity: 0,
            ovl_contracts: 0,
            tlt_activity: 0,
            nodes: 0,
            miners: 0,
            projects: 0,
        },
        assumption: "Counts are public totals only. This build ships zeros until a public aggregator exists. No per-user series is accepted.",
    }
}

pub fn parse_public_analytics(
    value: &Value,
) -> Result<PublicAnalyticsAggregates, CommunityTrustError> {
    let obj = value.as_object().ok_or_else(|| {
        CommunityTrustError::PrivateAnalytics("analytics must be an object".into())
    })?;
    for key in obj.keys() {
        if PRIVATE_ANALYTICS_KEYS.iter().any(|private| private == key) {
            return Err(CommunityTrustError::PrivateAnalytics(format!(
                "private field {key}"
            )));
        }
        if !PUBLIC_ANALYTICS_KEYS.iter().any(|public| public == key) {
            return Err(CommunityTrustError::PrivateAnalytics(format!(
                "unknown field {key}"
            )));
        }
    }
    for key in PUBLIC_ANALYTICS_KEYS {
        if !obj.contains_key(*key) {
            return Err(CommunityTrustError::PrivateAnalytics(format!(
                "missing field {key}"
            )));
        }
    }
    serde_json::from_value(value.clone())
        .map_err(|err| CommunityTrustError::PrivateAnalytics(err.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustAssumption {
    pub id: &'static str,
    pub title: &'static str,
    pub posture: &'static str,
    pub checks: &'static str,
    pub assumption: &'static str,
}

pub fn light_client_trust_assumptions() -> &'static [TrustAssumption] {
    &TRUST_ASSUMPTIONS
}

const TRUST_ASSUMPTIONS: [TrustAssumption; 10] = [
    TrustAssumption {
        id: "headers",
        title: "Headers",
        posture: "verified-locally",
        checks: "The device recomputes header hashes, checks selected-parent links, and binds the spine to the genesis hash from the node the wallet is using.",
        assumption: "RandomX work and the GHOSTDAG blue set are computed by that full node. A node can serve a self-consistent spine that is not the public network. This client does not compare a second node.",
    },
    TrustAssumption {
        id: "merkle",
        title: "Merkle",
        posture: "verified-locally",
        checks: "For a TLT transaction id, the device recomputes the pairwise SHA-256 Merkle path and the body-root fold into header.tx_root.",
        assumption: "The full node chooses which siblings to serve. A bad sibling fails the local check. The proof shows inclusion of that transaction id in that header. It does not prove an account balance or the absence of other transactions.",
    },
    TrustAssumption {
        id: "state_proofs",
        title: "State proofs",
        posture: "not-proven",
        checks: "The device rejects a balance or DRC object payload that claims header_proven.",
        assumption: "TLT, OVL, and DRC balances and DRC ledger objects are full-node state reads. The live header this client verifies has no state-root proof for those balances. OVL execution inclusion, when a binding is present, shows an id in a block body and does not verify an EVM receipt.",
    },
    TrustAssumption {
        id: "rpc",
        title: "RPC",
        posture: "node-reported",
        checks: "The wallet stores the RPC URL the user entered and checks the reported network id and genesis hash against that pairing.",
        assumption: "One RPC endpoint can hide transactions, balances, and votes. Bearer tokens stay in device storage. The node does not receive a mnemonic or a private key on submit. Submit sends an already signed transaction.",
    },
    TrustAssumption {
        id: "indexer",
        title: "Indexer",
        posture: "operator-reported",
        checks: "This client does not treat an indexer as a proof source.",
        assumption: "An indexer, if one is added later, is an operator database. It can be incomplete or wrong. Confirmation and inclusion still require the header and Merkle checks. This build does not query an indexer for trust.",
    },
    TrustAssumption {
        id: "community_api",
        title: "Community API",
        posture: "operator-reported",
        checks: "Local community search runs on device records. Seed examples are data, not a country enum, and the search box has no GPS input.",
        assumption: "A remote community API is not consensus. Hub, grant, mission, and analytics responses would be operator-reported. This build does not send identity exports or keys to a community API.",
    },
    TrustAssumption {
        id: "wallet",
        title: "Wallet",
        posture: "device-local",
        checks: "BIP-39 generation, AES-256-GCM vault sealing, and secp256k1 signing happen on the device. Desktop uses localStorage. Phone uses SecureStore.",
        assumption: "The vault password and mnemonic are not RPC parameters. Watch-only sessions have an account key and no spend key. A compromised RPC cannot sign by itself. A compromised device can.",
    },
    TrustAssumption {
        id: "passport",
        title: "Passport",
        posture: "device-local",
        checks: "The portable export is JSON built on the device. The builder refuses mnemonic, seed, and private-key fields. Service trust labels are attached to the bundle.",
        assumption: "Exporting a passport does not prove the issuer signature. This build does not re-verify hub signatures during export. Attestations are contribution evidence. They are not personhood and they are not Sybil resistance. Local preference fields stay on the device until the user copies the file.",
    },
    TrustAssumption {
        id: "governance",
        title: "Governance",
        posture: "mixed",
        checks: "Each proposal view carries a derived ON-CHAIN or ADVISORY badge. Advisory records cannot be given a binding passed result. Tallies shown here are counts.",
        assumption: "An advisory proposal does not change consensus. An on-chain tally read from one node is node-reported. This client does not re-check ballot signatures or chamber eligibility. Shipped status requires a commit or release link, and that link is not itself a consensus proof.",
    },
    TrustAssumption {
        id: "confirmation",
        title: "Confirmation",
        posture: "mixed",
        checks: "The wallet can watch a transaction until the node reports pending or confirmed, and it can verify a TLT Merkle proof locally when the node serves one.",
        assumption: "A single node's confirmed status is not network finality. Finality on this client is the PoW flag plus independent two-thirds OVL stake and two-thirds DRC stake, using the stake totals on the checkpoint. Those totals are not validator signature checks. If stake fields are absent, the block is not shown as finalized.",
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceTrustLabel {
    pub service: String,
    pub class: String,
    pub assumption: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicAttestationView {
    pub category: String,
    pub issuer_label: String,
    pub evidence_hash_hex: String,
    pub service_trust_class: String,
    pub service_trust_assumption: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicPassportExport {
    pub version: String,
    pub subject_address: Option<String>,
    pub attestations: Vec<PublicAttestationView>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalPrivatePrefs {
    pub version: String,
    pub language: Option<String>,
    pub region_query: Option<String>,
    pub interest_filters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableIdentityBundle {
    pub format: String,
    pub contains_private_keys: bool,
    pub public_passport: PublicPassportExport,
    pub local_private_prefs: LocalPrivatePrefs,
    pub service_trust: Vec<ServiceTrustLabel>,
}

#[derive(Debug, Clone)]
pub struct AttestationExportInput {
    pub category: String,
    pub issuer_label: String,
    pub evidence_hash_hex: String,
}

#[derive(Debug, Clone)]
pub struct IdentityExportInput {
    pub subject_address: Option<String>,
    pub attestations: Vec<AttestationExportInput>,
    pub language: Option<String>,
    pub region_query: Option<String>,
    pub interest_filters: Vec<String>,
}

pub fn export_portable_identity(input: IdentityExportInput) -> Result<String, CommunityTrustError> {
    let bundle = build_portable_identity(input)?;
    serde_json::to_string_pretty(&bundle)
        .map_err(|err| CommunityTrustError::IdentityExport(err.to_string()))
}

pub fn parse_portable_identity(bytes: &str) -> Result<PortableIdentityBundle, CommunityTrustError> {
    let value: Value = serde_json::from_str(bytes)
        .map_err(|err| CommunityTrustError::IdentityExport(err.to_string()))?;
    reject_secret_keys(&value)?;
    let bundle: PortableIdentityBundle = serde_json::from_value(value)
        .map_err(|err| CommunityTrustError::IdentityExport(err.to_string()))?;
    validate_portable_identity(&bundle)?;
    Ok(bundle)
}

fn build_portable_identity(
    input: IdentityExportInput,
) -> Result<PortableIdentityBundle, CommunityTrustError> {
    if input.attestations.len() > 64 {
        return Err(CommunityTrustError::IdentityExport(
            "too many attestations".into(),
        ));
    }
    let subject_address = match input.subject_address {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                reject_secret_text("subject", trimmed)?;
                if trimmed.chars().any(char::is_whitespace) || trimmed.chars().count() > 128 {
                    return Err(CommunityTrustError::IdentityExport(
                        "subject must be one public address".into(),
                    ));
                }
                Some(trimmed.to_string())
            }
        }
        None => None,
    };
    let language = match input.language {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                reject_secret_text("language", trimmed)?;
                if trimmed.chars().count() > 32 {
                    return Err(CommunityTrustError::IdentityExport(
                        "language is too long".into(),
                    ));
                }
                Some(trimmed.to_string())
            }
        }
        None => None,
    };
    let region_query = match input.region_query {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                reject_secret_text("region query", trimmed)?;
                if trimmed.chars().count() > 80 {
                    return Err(CommunityTrustError::IdentityExport(
                        "region query is too long".into(),
                    ));
                }
                Some(trimmed.to_string())
            }
        }
        None => None,
    };
    if input.interest_filters.len() > 16 {
        return Err(CommunityTrustError::IdentityExport(
            "too many interest filters".into(),
        ));
    }
    let mut interest_filters = Vec::with_capacity(input.interest_filters.len());
    for interest in input.interest_filters {
        let trimmed = interest.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 64 {
            return Err(CommunityTrustError::IdentityExport(
                "interest filter length".into(),
            ));
        }
        reject_secret_text("interest", trimmed)?;
        interest_filters.push(trimmed.to_string());
    }
    let mut attestations = Vec::with_capacity(input.attestations.len());
    for attestation in input.attestations {
        let category = attestation.category.trim();
        let issuer = attestation.issuer_label.trim();
        let hash = attestation.evidence_hash_hex.trim().to_ascii_lowercase();
        if category.is_empty()
            || issuer.is_empty()
            || category.chars().count() > 80
            || issuer.chars().count() > 80
        {
            return Err(CommunityTrustError::IdentityExport(
                "attestation label length".into(),
            ));
        }
        reject_secret_text("attestation", category)?;
        reject_secret_text("attestation", issuer)?;
        if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(CommunityTrustError::IdentityExport(
                "evidence hash must be 64 hex characters".into(),
            ));
        }
        attestations.push(PublicAttestationView {
            category: category.to_string(),
            issuer_label: issuer.to_string(),
            evidence_hash_hex: hash,
            service_trust_class: "hub_signed_unchecked".into(),
            service_trust_assumption: "Issuer signatures are not re-checked in this export.".into(),
        });
    }
    let bundle = PortableIdentityBundle {
        format: PORTABLE_IDENTITY_FORMAT.into(),
        contains_private_keys: false,
        public_passport: PublicPassportExport {
            version: PUBLIC_PASSPORT_VERSION.into(),
            subject_address,
            attestations,
            note: PASSPORT_EXPORT_NOTE.into(),
        },
        local_private_prefs: LocalPrivatePrefs {
            version: LOCAL_PREFS_VERSION.into(),
            language,
            region_query,
            interest_filters,
        },
        service_trust: service_trust_for_export(),
    };
    validate_portable_identity(&bundle)?;
    Ok(bundle)
}

fn service_trust_for_export() -> Vec<ServiceTrustLabel> {
    light_client_trust_assumptions()
        .iter()
        .map(|row| ServiceTrustLabel {
            service: row.id.to_string(),
            class: row.posture.to_string(),
            assumption: row.assumption.to_string(),
        })
        .collect()
}

fn validate_portable_identity(bundle: &PortableIdentityBundle) -> Result<(), CommunityTrustError> {
    if bundle.format != PORTABLE_IDENTITY_FORMAT {
        return Err(CommunityTrustError::IdentityExport(
            "unknown identity format".into(),
        ));
    }
    if bundle.contains_private_keys {
        return Err(CommunityTrustError::IdentityExport(
            "private keys are refused".into(),
        ));
    }
    if bundle.public_passport.version != PUBLIC_PASSPORT_VERSION {
        return Err(CommunityTrustError::IdentityExport(
            "unknown passport version".into(),
        ));
    }
    if bundle.public_passport.note.trim().is_empty() {
        return Err(CommunityTrustError::IdentityExport(
            "passport note is required".into(),
        ));
    }
    if bundle.local_private_prefs.version != LOCAL_PREFS_VERSION {
        return Err(CommunityTrustError::IdentityExport(
            "unknown prefs version".into(),
        ));
    }
    if bundle.service_trust.len() != TRUST_IDS.len() {
        return Err(CommunityTrustError::IdentityExport(
            "service trust labels are incomplete".into(),
        ));
    }
    for (label, id) in bundle.service_trust.iter().zip(TRUST_IDS) {
        if label.service != id
            || label.class.trim().is_empty()
            || label.assumption.trim().is_empty()
        {
            return Err(CommunityTrustError::IdentityExport(
                "service trust label is incomplete".into(),
            ));
        }
    }
    Ok(())
}

fn reject_secret_text(field: &str, text: &str) -> Result<(), CommunityTrustError> {
    if looks_like_seed_phrase(text) || contains_secret_marker(text) {
        return Err(CommunityTrustError::IdentityExport(format!(
            "{field} looks like key material and was refused"
        )));
    }
    Ok(())
}

fn looks_like_seed_phrase(text: &str) -> bool {
    text.split_whitespace().count() >= 12
}

fn contains_secret_marker(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "mnemonic",
        "xprv",
        "private_key",
        "private key",
        "seed phrase",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn reject_secret_keys(value: &Value) -> Result<(), CommunityTrustError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if SECRET_JSON_KEYS.iter().any(|secret| secret == key) {
                    return Err(CommunityTrustError::IdentityExport(format!(
                        "secret field {key}"
                    )));
                }
                reject_secret_keys(child)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                reject_secret_keys(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::{Proposal, ProposalKind, VoteTally};
    use agora_types::Address;

    fn narrative() -> TransparencyNarrative {
        TransparencyNarrative {
            why: "Publish the treasury impact before any spend.".into(),
            what_changes: "Records a community treasury intent.".into(),
            expected_cost: "0 TLT from emission.".into(),
            treasury_impact: "No automatic debit in this scaffold.".into(),
            eligibility: "Eligibility is the chamber rule the node reports.".into(),
            voting_start_slot: None,
            voting_end_slot: None,
            implementation_status: ImplementationStatus::NotStarted,
            commit_links: vec![],
            release_links: vec![],
        }
    }

    fn sample_proposal(status: ProposalStatus) -> Proposal {
        Proposal {
            id: 7,
            title: "Treasury intent".into(),
            summary: "Scaffold".into(),
            kind: ProposalKind::TextSignal,
            status,
            author: Address([4; 20]),
            deposit: 0,
            sponsors: vec![],
            created_slot: 1,
            voting_start_slot: Some(10),
            voting_end_slot: Some(20),
            timelock_end_slot: None,
            tally: VoteTally {
                yes: 3,
                no: 1,
                abstain: 2,
                no_with_veto: 0,
            },
            ballots: vec![],
            archon_assents: vec![],
        }
    }

    #[test]
    fn seeds_cover_requested_regions_without_a_country_enum() {
        let seeds = seed_local_communities();
        assert!(seeds
            .iter()
            .any(|row| row.country.as_deref() == Some("Greece")));
        assert!(seeds
            .iter()
            .any(|row| row.region.as_deref() == Some("Europe")));
        assert!(seeds
            .iter()
            .any(|row| row.region.as_deref() == Some("Asia")));
        assert!(seeds
            .iter()
            .any(|row| row.region.as_deref() == Some("Americas")));
        assert!(seeds
            .iter()
            .any(|row| row.city.as_deref() == Some("Athens")));
        let custom = local_community_from_json(
            r#"{"id":"example-city","name":"Example","country":"Not A Listed Country","languages":["en"]}"#,
        )
        .unwrap();
        assert_eq!(custom.country.as_deref(), Some("Not A Listed Country"));
    }

    #[test]
    fn manual_search_uses_text_and_has_no_gps_input() {
        assert!(!community_search_uses_device_location());
        assert!(MANUAL_REGION_SEARCH_NOTICE.contains("GPS is not requested"));
        let seeds = seed_local_communities();
        assert_eq!(search_local_communities(&seeds, "   ").len(), 4);
        assert_eq!(search_local_communities(&seeds, "Greece").len(), 1);
        assert_eq!(search_local_communities(&seeds, "athens").len(), 1);
        assert_eq!(search_local_communities(&seeds, "europe").len(), 1);
        assert_eq!(search_local_communities(&seeds, "Asia").len(), 1);
        assert_eq!(search_local_communities(&seeds, "Americas").len(), 1);
        assert!(search_local_communities(&seeds, "37.9838,23.7275").is_empty());
        assert!(search_local_communities(&seeds, "athens asia").is_empty());
        let err = local_community_from_json(
            r#"{"id":"gps","name":"GPS","country":"Greece","latitude":37.9}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("latitude"));
    }

    #[test]
    fn proposal_badges_are_derived_and_advisory_is_not_binding() {
        let on_chain =
            transparency_from_proposal(&sample_proposal(ProposalStatus::Voting), narrative())
                .unwrap();
        assert_eq!(authority_badge(on_chain.authority), "ON-CHAIN");
        assert_eq!(on_chain.current_votes.yes, 3);
        assert_eq!(on_chain.creator, hex::encode([4; 20]));
        let view = proposal_view(on_chain);
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains("ON-CHAIN"));

        let advisory = advisory_transparency(AdvisoryDraft {
            id: "advisory-note".into(),
            creator: "assembly scribe".into(),
            why: "Collect feedback.".into(),
            what_changes: "No consensus parameter changes.".into(),
            expected_cost: "None.".into(),
            treasury_impact: "None.".into(),
            voting_start_slot: 1,
            voting_end_slot: 2,
            eligibility: "Anyone reading the public square.".into(),
            current_votes: VoteSnapshot::default(),
            final_result: ProposalFinalResult::AdvisoryRecorded,
            implementation_status: ImplementationStatus::NotApplicable,
            commit_links: vec![],
            release_links: vec![],
        })
        .unwrap();
        assert_eq!(authority_badge(advisory.authority), "ADVISORY");

        let mut passed = advisory.clone();
        passed.final_result = ProposalFinalResult::Passed;
        assert!(validate_transparency(&passed).is_err());

        let mut shipped = advisory.clone();
        shipped.implementation_status = ImplementationStatus::Shipped;
        assert!(validate_transparency(&shipped)
            .unwrap_err()
            .to_string()
            .contains("commit or release"));
        shipped.commit_links = vec!["https://example.com/agora/commit/abc".into()];
        validate_transparency(&shipped).unwrap();
    }

    #[test]
    fn rewards_never_mint_and_tlt_emission_is_blocked() {
        let program = default_reward_program();
        let emission = program
            .iter()
            .find(|row| row.label == "TLT from emission")
            .unwrap();
        let blocked = evaluate_reward(emission);
        assert!(blocked.ui_blocked);
        assert!(!blocked.payable);
        assert!(!blocked.changes_tlt_emission);
        assert_eq!(blocked.supply_delta_base_units, 0);
        assert!(reward_control(&blocked).disabled);
        assert!(!reward_control(&blocked).submits_transaction);
        assert!(blocked.block_reason.unwrap().contains("emission"));

        let mut treasury = program
            .iter()
            .find(|row| row.label == "TLT from existing treasury")
            .unwrap()
            .clone();
        let still_blocked = evaluate_reward(&treasury);
        assert!(still_blocked.ui_blocked);
        treasury.source = RewardSource::ExistingTreasury {
            available_base_units: 5,
            amount_base_units: 5,
        };
        let payable = evaluate_reward(&treasury);
        assert!(payable.payable);
        assert!(!payable.ui_blocked);
        assert_eq!(payable.supply_delta_base_units, 0);
        assert!(!payable.changes_tlt_emission);
        assert!(!reward_control(&payable).submits_transaction);
        assert!(payable.funding_note.contains("already-issued TLT"));

        for config in program {
            let decision = evaluate_reward(&config);
            assert!(!decision.changes_tlt_emission);
            assert_eq!(decision.supply_delta_base_units, 0);
            assert!(!reward_control(&decision).submits_transaction);
        }
    }

    #[test]
    fn analytics_accept_only_public_totals() {
        let report = scaffold_public_analytics();
        assert!(!report.private_user_analytics);
        assert_eq!(report.aggregates.active_users, 0);
        let raw = serde_json::to_value(&report.aggregates).unwrap();
        parse_public_analytics(&raw).unwrap();
        let mut private = raw.clone();
        private["email"] = Value::String("a@example.com".into());
        assert!(parse_public_analytics(&private)
            .unwrap_err()
            .to_string()
            .contains("email"));
        let mut users = raw;
        users["active_users"] = serde_json::json!(["agora1secret"]);
        assert!(parse_public_analytics(&users).is_err());
    }

    #[test]
    fn identity_export_keeps_keys_off_the_bundle_and_labels_trust() {
        let json = export_portable_identity(IdentityExportInput {
            subject_address: Some("agora1example".into()),
            attestations: vec![AttestationExportInput {
                category: "code".into(),
                issuer_label: "Athens hub".into(),
                evidence_hash_hex: "ab".repeat(32),
            }],
            language: Some("en".into()),
            region_query: Some("Greece".into()),
            interest_filters: vec!["governance".into()],
        })
        .unwrap();
        assert!(!json.contains("\"mnemonic\""));
        assert!(!json.contains("\"private_key\""));
        assert!(!json.contains("\"xprv\""));
        let bundle = parse_portable_identity(&json).unwrap();
        assert!(!bundle.contains_private_keys);
        assert_eq!(bundle.service_trust.len(), 10);
        assert_eq!(
            bundle.local_private_prefs.region_query.as_deref(),
            Some("Greece")
        );
        assert!(bundle.public_passport.note.contains("personhood"));

        let refused = export_portable_identity(IdentityExportInput {
            subject_address: None,
            attestations: vec![],
            language: None,
            region_query: Some(
                "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu".into(),
            ),
            interest_filters: vec![],
        });
        assert!(refused.unwrap_err().to_string().contains("key material"));

        let mut tampered: Value = serde_json::from_str(&json).unwrap();
        tampered["mnemonic"] = Value::String("abandon abandon".into());
        assert!(parse_portable_identity(&tampered.to_string())
            .unwrap_err()
            .to_string()
            .contains("mnemonic"));
        tampered.as_object_mut().unwrap().remove("mnemonic");
        tampered["contains_private_keys"] = Value::Bool(true);
        assert!(parse_portable_identity(&tampered.to_string())
            .unwrap_err()
            .to_string()
            .contains("private keys"));
    }

    #[test]
    fn trust_assumptions_are_complete_and_visible() {
        let rows = light_client_trust_assumptions();
        let ids: Vec<&str> = rows.iter().map(|row| row.id).collect();
        assert_eq!(ids, TRUST_IDS);
        for row in rows {
            assert!(!row.assumption.trim().is_empty());
            assert!(!row.checks.trim().is_empty());
            assert!(row.assumption.len() > 40);
        }
    }
}
