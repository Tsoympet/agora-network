//! Civic governance for Agora Network.
//!
//! ## What this crate provides
//!
//! 1. **Constitution v1** — higher-law text + content hash (`constitution`)
//! 2. **Elected ranks** — Archons, Bouleutai, Tamiai (`ranks`, `office`)
//! 3. **Voting chambers** — Ecclesia / Boule / Archon Collegium (`chamber`)
//! 4. **Proposal lifecycle** — deposit → vote → tally → timelock → execute (`engine`)
//! 5. **Community board** — EOS-style forum topics + constitution acks (`community`)
//! 6. **Durable snapshot** — JSON for node Meta CF (`persist`)
//! 7. **Launch-security math** — quadratic votes + 5% whale cap (`quadratic`, `whale`)
//! 8. **Community trust** — local attributes, proposal transparency, rewards,
//!    public aggregates, and portable identity (`community_trust`)
//!
//! See `docs/governance/CONSTITUTION.md`, `docs/governance/COMMUNITY.md`,
//! and `docs/community/COMMUNITY_TRUST.md`.

mod authorization;
mod chamber;
mod community;
mod community_protocol;
mod community_trust;
mod constitution;
mod engine;
mod error;
mod math;
mod office;
mod params;
mod persist;
mod proposal;
mod quadratic;
mod ranks;
mod trident;
mod views;
mod whale;

pub use authorization::{
    authorization_for, authorization_for_class, primary_chamber, proposal_class,
    ProposalAuthorization, ProposalClass,
};
pub use chamber::VotingChamber;
pub use community::{CommunityBoard, ConstitutionAck, ForumTopic, TopicCategory};
pub use community_protocol::{
    CommunityProtocolError, ConflictReviewStatus, GrantKind, GrantMilestone, GrantRecord,
    GrantStatus, HubAccreditationStatus, HubKind, HubRecord, MilestoneStatus, MissionRecord,
    MissionStatus,
};
pub use community_trust::{
    advisory_transparency, authority_badge, community_search_uses_device_location,
    default_reward_program, evaluate_reward, export_portable_identity,
    light_client_trust_assumptions, local_community_from_json, parse_portable_identity,
    parse_public_analytics, proposal_view, reward_control, scaffold_public_analytics,
    search_local_communities, seed_local_communities, transparency_from_proposal,
    CommunityTrustError, ImplementationStatus, LocalCommunity, PortableIdentityBundle,
    ProposalAuthority, ProposalFinalResult, ProposalTransparency, ProposalTransparencyView,
    PublicAnalyticsAggregates, RewardConfig, RewardDecision, RewardKind, RewardSource,
    TransparencyNarrative, TrustAssumption, MANUAL_REGION_SEARCH_NOTICE,
};
pub use constitution::{
    constitution_v1_hash_hex, hash_constitution_body, EnactedConstitution, CONSTITUTION_V1_BODY,
    CONSTITUTION_V1_ID,
};
pub use engine::GovernanceState;
pub use error::GovernanceError;
pub use math::isqrt;
pub use office::{OfficeBoard, OfficeSeat};
pub use params::GovernanceParams;
pub use persist::{CivicSnapshot, CIVIC_META_KEY};
pub use proposal::{
    Ballot, ParameterScope, Proposal, ProposalKind, ProposalStatus, VoteChoice, VoteTally,
};
pub use quadratic::{
    effective_votes_for, max_power_share_bps, quadratic_votes, tally_quadratic_votes,
    total_effective_power, EffectiveVote, VoterBalance,
};
pub use ranks::CivicRank;
pub use trident::{
    trident_approval_matrix, trident_policy_catalog, TimelockClass, TridentApprovalMatrix,
    TridentChamber, TridentProposalClass,
};
pub use views::{
    civic_overview_json, list_proposals_json, list_topics_json, office_json, proposal_json,
    topic_json,
};
pub use whale::{apply_whale_cap, WhaleCapConfig};
