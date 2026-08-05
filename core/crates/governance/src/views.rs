//! Explorer-friendly JSON views for civic governance.

use serde_json::{json, Value};

use crate::chamber::primary_chamber;
use crate::community::{CommunityBoard, ForumTopic};
use crate::engine::GovernanceState;
use crate::office::OfficeSeat;
use crate::persist::CivicSnapshot;
use crate::proposal::Proposal;

pub fn civic_overview_json(snap: &CivicSnapshot) -> Value {
    json!({
        "constitution": {
            "id": snap.governance.constitution.id,
            "content_hash": snap.governance.constitution.content_hash_hex(),
            "body_markdown": snap.governance.constitution.body_markdown,
        },
        "params": snap.governance.params,
        "offices": snap.governance.offices.seats.iter().map(office_json).collect::<Vec<_>>(),
        "proposal_count": snap.governance.proposals.len(),
        "topic_count": snap.community.topics.len(),
        "constitution_ack_count": snap.community.constitution_acks.len(),
        "ecclesia_eligible_power": snap.governance.ecclesia_eligible_power,
    })
}

pub fn office_json(seat: &OfficeSeat) -> Value {
    json!({
        "rank": seat.rank,
        "title": seat.rank.title(),
        "greek": seat.rank.greek(),
        "seat_index": seat.seat_index,
        "holder": seat.holder.map(|a| a.to_bech32()),
        "elected_slot": seat.elected_slot,
        "term_end_slot": seat.term_end_slot,
    })
}

pub fn proposal_json(p: &Proposal) -> Value {
    json!({
        "id": p.id,
        "title": p.title,
        "summary": p.summary,
        "kind": p.kind,
        "status": p.status,
        "chamber": primary_chamber(&p.kind),
        "author": p.author.to_bech32(),
        "deposit": p.deposit,
        "sponsors": p.sponsors.iter().map(|a| a.to_bech32()).collect::<Vec<_>>(),
        "created_slot": p.created_slot,
        "voting_start_slot": p.voting_start_slot,
        "voting_end_slot": p.voting_end_slot,
        "timelock_end_slot": p.timelock_end_slot,
        "tally": p.tally,
        "ballots": p.ballots.iter().map(|b| json!({
            "voter": b.voter.to_bech32(),
            "choice": b.choice,
            "weight": b.weight,
        })).collect::<Vec<_>>(),
        "archon_assents": p.archon_assents.iter().map(|a| a.to_bech32()).collect::<Vec<_>>(),
    })
}

pub fn list_proposals_json(gov: &GovernanceState, limit: usize) -> Value {
    let mut ids: Vec<u64> = gov.proposals.keys().copied().collect();
    ids.sort_by(|a, b| b.cmp(a));
    let items: Vec<Value> = ids
        .into_iter()
        .take(limit.max(1))
        .filter_map(|id| gov.proposals.get(&id).map(proposal_json))
        .collect();
    json!({ "count": items.len(), "proposals": items })
}

pub fn topic_json(t: &ForumTopic) -> Value {
    json!({
        "id": t.id,
        "author": t.author.to_bech32(),
        "title": t.title,
        "body": t.body,
        "category": t.category,
        "created_slot": t.created_slot,
        "linked_proposal_id": t.linked_proposal_id,
    })
}

pub fn list_topics_json(board: &CommunityBoard, limit: usize) -> Value {
    let topics: Vec<Value> = board
        .list_topics(limit)
        .into_iter()
        .map(topic_json)
        .collect();
    json!({ "count": topics.len(), "topics": topics })
}
