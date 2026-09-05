# Trident Block 0 commitment

**Maturity:** Scaffold. This is not a node boot path and does not claim public-testnet readiness.

`core/crates/state-machine/src/block_zero.rs` defines the versioned,
deterministic Borsh manifest that a future Trident Block 0 transition must
materialize. Preparation is accepted only from a freeze-ready v3 artifact.

Manifest version 2 is a pre-freeze break. It binds the artifact `chain_id` and
network fingerprint into both `TridentBlockZeroState` and
`TridentBlockZeroCommitment` so those identities cannot be omitted from the
future header/datadir contract.

The manifest commits:

- chain ID and network fingerprint, together with the artifact and consensus-policy identities;
- every TLT, OVL, and DRC initial allocation and vesting lock;
- maximum, allocated, treasury, staking-reserve, and unissued supply buckets;
- all three protocol treasuries, including their artifact-selected controls;
- independent epoch-zero OVL and DRC validator sets, public keys, withdrawal
  addresses, funded self-bonds, set policies, totals, and snapshot commitments;
- the constitution and emergency-policy hashes;
- an explicitly unfinalized initial checkpoint state with no signatures or PoW
  satisfaction.

Collections are sorted by wire asset and address identities before hashing.
`TridentBlockZeroState::verify` checks supply conservation, allocation-backed
vesting and validator self-bonds, independent validator-set identities, fingerprint
consistency, and the initial finality state. `verified_borsh_payload` then performs
an exact Borsh round trip and rechecks the state root.

## Candidate storage envelope

Explicit Meta keys under `meta/trident_block_zero/` hold a versioned Borsh
envelope (`TRIDENT_BLOCK_ZERO_STORAGE_VERSION = 1`, independent of live
`SCHEMA_VERSION`). The envelope preserves the complete manifest, canonical
payload, commitment, commitment hash, artifact identity, consensus policy hash,
network fingerprint, and chain ID.

`TridentBlockZeroState::stage_verified_store_batch` stages every key in one
`WriteBatch`, applies that batch only to a copy-on-write overlay, rereads the
bytes, and returns the batch for a future loader. Missing, malformed,
inconsistent, duplicate, or mismatched records are rejected before any durable
write. The current `GenesisBuilder` ignition path never writes these keys and
does not consume the checked reader.

## Why the loader remains disabled

The abstraction still does not construct a [`agora_types::Block`], materialize
live UTXO/account balances, or run inside `agora-node`. A partial boot path
would be unsafe because:

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
7. Datadir identity currently checks only the legacy genesis hash. The candidate
   Block 0 envelope now preserves the extra identities, but boot still must bind
   artifact identity, policy hash, Block 0 commitment, state root, chain ID, and
   network fingerprint before P2P identity generation or any networking/RPC
   startup.

The next safe change is a version-gated header encoding plus lossless live-state
mappings that can be appended to the already-verified Block 0 batch. Commit only
when the recomputed root equals the Block 0 header commitment. Until all steps
exist together, `AGORA_GENESIS_FILE` remains the frozen v2 loader only.
