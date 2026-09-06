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
envelope (`TRIDENT_BLOCK_ZERO_STORAGE_VERSION = 2`, independent of live
`SCHEMA_VERSION`). The envelope preserves the complete manifest, canonical
payload, commitment, commitment hash, artifact identity, consensus policy hash,
network fingerprint, chain ID, and bound datadir identity.

`TridentDatadirIdentity` is a separately versioned Borsh record under
`meta/trident_datadir_identity/`. It binds the chain ID, network fingerprint,
artifact identity, consensus-policy hash, Block 0 commitment, committed state
root, and the Trident header network identity. When a fully specified offline
header is available, it also binds that header's canonical hash; no ceremony
value is defaulted when the hash is absent.

`TridentBlockZeroState::stage_verified_store_batch` places the envelope and
identity bytes in one `WriteBatch`, applies that batch only to a copy-on-write
overlay, and rereads every byte. `persist_verified_store_record` commits that
same batch atomically and performs a durable reread. Reopen verification decodes
both records, requires canonical Borsh round trips, compares the independently
stored identity bytes with the copy inside the Block 0 envelope, and can compare
the entire actual identity byte-for-byte with a caller-supplied expected
identity. Missing, malformed, inconsistent, duplicate, partial, tampered, or
mismatched records fail closed.

The legacy `GenesisBuilder` load paths reject any complete or partial Trident
Block 0/datadir marker, even when a valid v2 genesis hash is also present.
`agora-node` completes this storage preflight before it calls the libp2p
identity loader. A candidate persisted by offline tooling therefore cannot be
silently opened, ignored, or overwritten by the v2 node.

## Offline header bridge

`TridentBlockZeroCommitment::to_offline_trident_header` now converts a
self-consistent commitment into the separate versioned `TridentHeader` type.
The conversion fixes no ceremony values: callers must provide timestamp,
difficulty, nonce, and a nonzero concrete-body root. It repeats and verifies
the Block 0 commitment hash, artifact identity, consensus-policy hash, Trident
protocol/state-transition versions, and state root. Block 0 parents must be
empty. This type has no conversion to the current `Block`, no storage key, and
no loader, mining, consensus, RPC, or P2P consumer.

## Why the loader remains disabled

The abstraction still does not construct a [`agora_types::Block`], materialize
live UTXO/account balances, or run inside `agora-node`. A partial boot path
would be unsafe because:

1. The new header encoding is offline-only. A concrete Trident body format,
   body-root derivation, PoW hash rule, and explicit runtime protocol gate still
   need specification and wiring; the frozen v2 `BlockHeader`/`Block` path
   cannot be repurposed.
2. TLT artifact allocations still need a lossless Block 0 transaction/UTXO
   mapping, while OVL/DRC allocations need an atomic account mapping whose
   composed root exactly equals the header state root.
3. Runtime treasury records do not preserve artifact treasury controls, and the
   governance store currently initializes compiled defaults rather than the
   artifact-selected constitution and emergency-policy hashes.
4. No canonical vesting store or lock enforcement exists.
5. Validator runtime records require per-validator commission and metadata
   values that the v3 artifact does not contain. Those values must be selected
   by the ceremony schema, not invented by the loader.
6. The initial finality record and epoch-zero snapshots need explicit persistent
   keys and inclusion in the live composed state root.
7. Future Trident startup must call `verify_trident_datadir_identity` with the
   identity derived from its independently verified artifact and concrete
   header before loading a libp2p key or binding RPC. This prerequisite provides
   that fail-closed comparison, but no Trident runtime path invokes it yet.

The remaining live-state blocker is the lossless materialization and atomic
root check: define the concrete Block 0 body/UTXOs, account and treasury
records, vesting locks, complete validator/finality records, and append them to
the already-verified batch only when the recomputed live composed root equals
the offline header. Explicit consensus/PoW/storage/P2P/RPC activation gates
must then consume that verified state without changing v2 identities. Until
those pieces exist together, `AGORA_GENESIS_FILE` remains the frozen v2 loader
only.
