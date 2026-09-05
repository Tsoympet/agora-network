# Trident Block 0 commitment

**Maturity:** Scaffold. This is not a node boot path and does not claim public-testnet readiness.

`core/crates/state-machine/src/block_zero.rs` defines the versioned,
deterministic Borsh manifest that a future Trident Block 0 transition must
materialize. Preparation is accepted only from a freeze-ready v3 artifact.

The manifest commits:

- every TLT, OVL, and DRC initial allocation and vesting lock;
- maximum, allocated, treasury, staking-reserve, and unissued supply buckets;
- all three protocol treasuries, including their artifact-selected controls;
- independent epoch-zero OVL and DRC validator sets, public keys, withdrawal
  addresses, funded self-bonds, set policies, totals, and snapshot commitments;
- the consensus-policy, constitution, emergency-policy, and artifact identities;
- an explicitly unfinalized initial checkpoint state with no signatures or PoW
  satisfaction.

Collections are sorted by wire asset and address identities before hashing.
`TridentBlockZeroState::verify` checks supply conservation, allocation-backed
vesting and validator self-bonds, independent validator-set identities, and the
initial finality state. `verified_borsh_payload` then performs an exact Borsh
round trip and rechecks the state root.

## Why the loader remains disabled

The abstraction intentionally has no storage writer and is not used by
`agora-node`. A partial boot path would be unsafe because:

1. `BlockHeader` commits only `tx_root`; adding a field with derived Borsh would
   change the frozen v2 Block 0 hash unless serialization is explicitly
   version-gated.
2. TLT artifact allocations still need a specified Block 0 transaction/UTXO
   mapping, while OVL/DRC allocations need an atomic account mapping.
3. Runtime treasury records do not preserve artifact treasury controls, and the
   governance store currently initializes compiled defaults rather than the
   artifact-selected constitution and emergency-policy hashes.
4. No canonical vesting store or lock enforcement exists.
5. Validator runtime records require per-validator commission and metadata
   values that the v3 artifact does not contain. Those values must be selected
   by the ceremony schema, not invented by the loader.
6. The initial finality record and epoch-zero snapshots need explicit persistent
   keys and inclusion in the live composed state root.
7. Datadir identity currently checks only the legacy genesis hash. Trident must
   bind the artifact identity, policy hash, Block 0 commitment, state root,
   chain ID, and network fingerprint before P2P identity generation or any
   networking/RPC startup.

The next safe change is to specify those lossless store mappings and a
version-gated header encoding, then stage all resulting writes in one
`WriteBatch`, verify the staged state through a copy-on-write view, and commit
only when the recomputed root equals the Block 0 header commitment. Until all
steps exist together, `AGORA_GENESIS_FILE` remains the frozen v2 loader only.
