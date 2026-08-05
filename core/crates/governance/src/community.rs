//! Community gathering layer — EOS forum / Athens Agora patterns.
//!
//! Borrowed ideas:
//! - **EOS `eosio.forum`**: on-chain proposal/discussion topics anyone can post
//! - **EOS constitution ack**: users bind to the constitution content hash
//! - **EOS Worker Proposal System**: treasury ideas surfaced as community topics
//!   before / alongside formal `TreasurySpend` proposals
//! - **Classical Agora**: public square for signals and assembly notices

use agora_types::Address;
use serde::{Deserialize, Serialize};

use crate::constitution::{constitution_v1_hash_hex, CONSTITUTION_V1_ID};
use crate::GovernanceError;

/// Topic category for the public square (community board).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopicCategory {
    /// General discussion (EOS forum-style free post).
    Discussion,
    /// Soft signal / poll before a formal Ecclesia proposal.
    Signal,
    /// Worker / treasury idea (feeds later `TreasurySpend`).
    WorkerIdea,
    /// Assembly notice from Boule / Archons.
    AssemblyNotice,
}

impl TopicCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discussion => "discussion",
            Self::Signal => "signal",
            Self::WorkerIdea => "worker_idea",
            Self::AssemblyNotice => "assembly_notice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumTopic {
    pub id: u64,
    pub author: Address,
    pub title: String,
    pub body: String,
    pub category: TopicCategory,
    pub created_slot: u64,
    /// Optional link to a formal governance proposal id once elevated.
    pub linked_proposal_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstitutionAck {
    pub address: Address,
    pub constitution_id: String,
    pub constitution_hash_hex: String,
    pub acked_slot: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityBoard {
    pub topics: Vec<ForumTopic>,
    pub next_topic_id: u64,
    pub constitution_acks: Vec<ConstitutionAck>,
}

impl Default for CommunityBoard {
    fn default() -> Self {
        Self {
            topics: Vec::new(),
            next_topic_id: 1,
            constitution_acks: Vec::new(),
        }
    }
}

impl CommunityBoard {
    pub fn post_topic(
        &mut self,
        author: Address,
        title: impl Into<String>,
        body: impl Into<String>,
        category: TopicCategory,
        created_slot: u64,
    ) -> Result<u64, GovernanceError> {
        let title = title.into().trim().to_string();
        let body = body.into().trim().to_string();
        if title.is_empty() || body.is_empty() {
            return Err(GovernanceError::InvalidTopic);
        }
        if title.len() > 200 || body.len() > 8_000 {
            return Err(GovernanceError::InvalidTopic);
        }
        let id = self.next_topic_id;
        self.next_topic_id = self
            .next_topic_id
            .checked_add(1)
            .ok_or(GovernanceError::Overflow)?;
        self.topics.push(ForumTopic {
            id,
            author,
            title,
            body,
            category,
            created_slot,
            linked_proposal_id: None,
        });
        Ok(id)
    }

    pub fn link_topic_to_proposal(
        &mut self,
        topic_id: u64,
        proposal_id: u64,
    ) -> Result<(), GovernanceError> {
        let topic = self
            .topics
            .iter_mut()
            .find(|t| t.id == topic_id)
            .ok_or(GovernanceError::UnknownTopic)?;
        topic.linked_proposal_id = Some(proposal_id);
        Ok(())
    }

    /// EOS-style constitution acknowledgment (binds address to current hash).
    pub fn acknowledge_constitution(
        &mut self,
        address: Address,
        constitution_id: impl Into<String>,
        constitution_hash_hex: impl Into<String>,
        acked_slot: u64,
    ) {
        let constitution_id = constitution_id.into();
        let constitution_hash_hex = constitution_hash_hex.into();
        if let Some(existing) = self
            .constitution_acks
            .iter_mut()
            .find(|a| a.address == address)
        {
            existing.constitution_id = constitution_id;
            existing.constitution_hash_hex = constitution_hash_hex;
            existing.acked_slot = acked_slot;
        } else {
            self.constitution_acks.push(ConstitutionAck {
                address,
                constitution_id,
                constitution_hash_hex,
                acked_slot,
            });
        }
    }

    pub fn has_acked_current_v1(&self, address: Address) -> bool {
        let want = constitution_v1_hash_hex();
        self.constitution_acks.iter().any(|a| {
            a.address == address
                && a.constitution_id == CONSTITUTION_V1_ID
                && a.constitution_hash_hex == want
        })
    }

    pub fn list_topics(&self, limit: usize) -> Vec<&ForumTopic> {
        let mut refs: Vec<_> = self.topics.iter().collect();
        refs.sort_by(|a, b| b.id.cmp(&a.id));
        refs.into_iter().take(limit.max(1)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    #[test]
    fn post_and_ack_roundtrip() {
        let mut board = CommunityBoard::default();
        let id = board
            .post_topic(
                addr(1),
                "Build explorers",
                "We should fund explorer hosting",
                TopicCategory::WorkerIdea,
                10,
            )
            .unwrap();
        assert_eq!(id, 1);
        board.acknowledge_constitution(
            addr(1),
            CONSTITUTION_V1_ID,
            constitution_v1_hash_hex(),
            11,
        );
        assert!(board.has_acked_current_v1(addr(1)));
        assert!(!board.has_acked_current_v1(addr(2)));
    }
}
