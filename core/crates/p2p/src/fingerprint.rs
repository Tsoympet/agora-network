//! Network fingerprint for P2P mesh isolation.
//!
//! Peers that share a textual network label (`testnet`) but disagree on genesis
//! or consensus policy must not share gossip topics / request protocols.

use agora_types::Hash;

/// Wire versions included in the fingerprint so signing / state-transition
/// upgrades force a new mesh.
///
/// These remain the **v2 / pre-Trident** constants so the frozen TLT testnet mesh
/// is unchanged. Trident meshes use [`trident_network_fingerprint`].
pub const TX_SIGNING_VERSION: &str = "agora-tx-v1";
pub const STATE_TRANSITION_VERSION: &str = "agora-utxo-virtual-v1";
pub const PROTOCOL_VERSION: u32 = 1;

/// Trident fingerprint domain + versions (distinct mesh from v2).
pub const TRIDENT_NET_FP_DOMAIN: &[u8] = b"agora-trident-net-fp-v1";
/// v23 activates the root-committed DRC object/owner/accepted-operation index.
pub const TRIDENT_PROTOCOL_VERSION: u32 = 23;
pub const TRIDENT_TX_SIGNING_VERSION: &str = "agora-trident-tx-v9";
pub const TRIDENT_STATE_TRANSITION_VERSION: &str = "agora-trident-state-v21";
pub const TRIDENT_CONSENSUS_POLICY_VERSION: &str = "agora-trident-consensus-v2";

/// Canonical network fingerprint hash (pre-Trident / genesis v2).
pub fn network_fingerprint(chain_id: &str, genesis: &Hash, consensus_policy_hash: &Hash) -> Hash {
    Hash::hash_borsh(&(
        b"agora-net-fp-v1",
        PROTOCOL_VERSION,
        chain_id,
        genesis.as_bytes(),
        consensus_policy_hash.as_bytes(),
        TX_SIGNING_VERSION,
        STATE_TRANSITION_VERSION,
    ))
}

/// Trident L1 network fingerprint.
///
/// Any change to chain id, genesis/identity digest, consensus policy, or the
/// Trident version strings must produce a new mesh tag.
pub fn trident_network_fingerprint(
    chain_id: &str,
    genesis_or_identity: &Hash,
    consensus_policy_hash: &Hash,
) -> Hash {
    Hash::hash_borsh(&(
        TRIDENT_NET_FP_DOMAIN,
        TRIDENT_PROTOCOL_VERSION,
        chain_id,
        genesis_or_identity.as_bytes(),
        consensus_policy_hash.as_bytes(),
        TRIDENT_TX_SIGNING_VERSION,
        TRIDENT_STATE_TRANSITION_VERSION,
        TRIDENT_CONSENSUS_POLICY_VERSION,
    ))
}

/// Short hex prefix used in gossip topic / protocol names (16 hex chars).
pub fn fingerprint_topic_tag(fp: &Hash) -> String {
    fp.to_hex()[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_changes_with_policy() {
        let genesis = Hash::hash_borsh(&"genesis");
        let a = network_fingerprint("agora-testnet-1", &genesis, &Hash::hash_borsh(&1u64));
        let b = network_fingerprint("agora-testnet-1", &genesis, &Hash::hash_borsh(&2u64));
        assert_ne!(a, b);
        assert_eq!(fingerprint_topic_tag(&a).len(), 16);
    }

    #[test]
    fn trident_fingerprint_distinct_from_v2_and_version_sensitive() {
        let genesis = Hash::hash_borsh(&"genesis");
        let policy = Hash::hash_borsh(&1u64);
        let v2 = network_fingerprint("agora-trident-testnet-1", &genesis, &policy);
        let t1 = trident_network_fingerprint("agora-trident-testnet-1", &genesis, &policy);
        assert_ne!(v2, t1);
        assert_eq!(
            t1.to_hex(),
            "250e50a1bfcaaa6efa938b672817342968508c8eebf703715847d3c7f8adf8a0"
        );
        let t2 = trident_network_fingerprint("agora-trident-testnet-2", &genesis, &policy);
        assert_ne!(t1, t2);

        let prior = Hash::hash_borsh(&(
            TRIDENT_NET_FP_DOMAIN,
            8u32,
            "agora-trident-testnet-1",
            genesis.as_bytes(),
            policy.as_bytes(),
            "agora-trident-tx-v3",
            "agora-trident-state-v9",
            TRIDENT_CONSENSUS_POLICY_VERSION,
        ));
        assert_ne!(t1, prior);
        assert_eq!(TRIDENT_PROTOCOL_VERSION, 23);
        assert_eq!(TRIDENT_TX_SIGNING_VERSION, "agora-trident-tx-v9");
        assert_eq!(TRIDENT_STATE_TRANSITION_VERSION, "agora-trident-state-v21");
    }

    #[test]
    fn trident_genesis_and_p2p_fingerprint_versions_match() {
        use agora_state_machine::TridentGenesisArtifact;

        const DRAFT: &str =
            include_str!("../../../../docs/genesis/trident.testnet.genesis.draft.json");
        let artifact = TridentGenesisArtifact::from_json(DRAFT).unwrap();
        let p2p_fingerprint = trident_network_fingerprint(
            &artifact.chain_id,
            &artifact.consensus_identity_hash(),
            &artifact.consensus_policy_hash(),
        );

        assert_eq!(
            TRIDENT_PROTOCOL_VERSION,
            agora_state_machine::TRIDENT_PROTOCOL_VERSION
        );
        assert_eq!(
            TRIDENT_TX_SIGNING_VERSION,
            agora_state_machine::TRIDENT_TX_SIGNING_VERSION
        );
        assert_eq!(
            TRIDENT_STATE_TRANSITION_VERSION,
            agora_state_machine::TRIDENT_STATE_TRANSITION_VERSION
        );
        assert_eq!(
            TRIDENT_CONSENSUS_POLICY_VERSION,
            agora_state_machine::TRIDENT_CONSENSUS_POLICY_VERSION
        );
        assert_eq!(p2p_fingerprint, artifact.compute_network_fingerprint());
    }
}
