//! Versioned Agora Constitution (higher law).

use sha2::{Digest, Sha256};

/// In-protocol constitution identifier for v1.
pub const CONSTITUTION_V1_ID: &str = "constitution-v1";

/// Canonical markdown body embedded for hash stability.
///
/// Keep in sync with `docs/governance/CONSTITUTION.md` (substantive articles).
/// The docs file may add editorial framing; the hash below is over this exact
/// string so nodes agree on enacted higher law.
pub const CONSTITUTION_V1_BODY: &str = r#"# Constitution of the Agora Network
Version: 1
Id: constitution-v1
Binding asset: TLT

## Article I — Sovereignty of the Ecclesia
The Ecclesia is the assembly of all TLT holders. Final authority over ordinary proposals, elections, treasury spends, and constitution amendments rests with the Ecclesia unless this charter assigns a prior chamber vote. Vote weight is quadratic after a 5% whale cap.

## Article II — The Boule
The Boule is a standing council of 21 elected Bouleutai. Boule chamber voting is one seat, one vote.

## Article III — The Archons
Elected ranks: Archon Eponymous (1), Archon Basileus (1), Archon Polemarch (1), Bouleutes (21), Tamias (3).

## Article IV — Voting chambers
Ecclesia (all holders, quadratic), Boule (seated Bouleutai, 1:1), ArchonCollegium (three Archons, 1:1). Proposal kinds map to a primary chamber; ConstitutionAmendment requires Basileus or 2-of-3 Archon assent; TreasurySpend requires Tamias sponsorship; EmergencyAction starts in ArchonCollegium.

## Article V — Proposal lifecycle
Draft → Deposit → Voting → Tally → Passed/Rejected/FailedQuorum/Vetoed → Timelock → Executed.

## Article VI — Elections
RankElection proposals are decided in the Ecclesia.

## Article VII — Impeachment & vacancy
RankImpeachment removes a seat; Archon vacancies require Ecclesia election.

## Article VIII — Amending this Constitution
ConstitutionAmendment with heightened quorum/threshold plus Archon assent; content hash stored as constitution_hash.

## Article IX — Scope
TLT civic governance; L2/L3 operator sets are not Ecclesia ranks.

## Article X — Enactment
Enacted when governance state records constitution-v1 and matching constitution_hash.
"#;

/// SHA-256 hex of [`CONSTITUTION_V1_BODY`].
pub fn constitution_v1_hash_hex() -> String {
    hex::encode(Sha256::digest(CONSTITUTION_V1_BODY.as_bytes()))
}

/// SHA-256 raw bytes of an arbitrary constitution body.
pub fn hash_constitution_body(body: &str) -> [u8; 32] {
    Sha256::digest(body.as_bytes()).into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnactedConstitution {
    pub id: String,
    pub body_markdown: String,
    pub content_hash: [u8; 32],
}

impl EnactedConstitution {
    pub fn v1() -> Self {
        Self {
            id: CONSTITUTION_V1_ID.to_string(),
            body_markdown: CONSTITUTION_V1_BODY.to_string(),
            content_hash: hash_constitution_body(CONSTITUTION_V1_BODY),
        }
    }

    pub fn from_body(id: impl Into<String>, body_markdown: impl Into<String>) -> Self {
        let body_markdown = body_markdown.into();
        let content_hash = hash_constitution_body(&body_markdown);
        Self {
            id: id.into(),
            body_markdown,
            content_hash,
        }
    }

    pub fn content_hash_hex(&self) -> String {
        hex::encode(self.content_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_hash_is_stable() {
        let a = constitution_v1_hash_hex();
        let b = EnactedConstitution::v1().content_hash_hex();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(
            a,
            "82ef5b8fd16f576cc5fd07f702637b29915d863f261b39ecd2fdce7b79577118"
        );
    }
}
