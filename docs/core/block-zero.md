# Trident Block 0 commitment

**Maturity:** Scaffold. This is not a node boot path and does not claim public-testnet readiness.

`core/crates/state-machine/src/block_zero.rs` defines the versioned,
deterministic Borsh manifest that a future Trident Block 0 transition must
materialize. Preparation is accepted only from a freeze-ready v3 artifact.

Manifest version 4 is a pre-freeze break. It retains the chain ID,
network-fingerprint, and complete validator registration binding, adds the
artifact timestamp/difficulty and validator reward-drip policy needed by live
records, and separates the canonical manifest root from the composed live-state
root in `TridentBlockZeroCommitment`.

The manifest commits:

- chain ID and network fingerprint, together with the artifact and consensus-policy identities;
- every TLT, OVL, and DRC initial allocation and vesting lock, including an
  explicit `linear_from_start_with_cliff_v1` release policy;
- maximum, allocated, treasury, staking-reserve, and unissued supply buckets;
- all three protocol treasuries, including their artifact-selected controls;
- independent epoch-zero OVL and DRC validator sets, secp256k1 public keys,
  withdrawal addresses, funded self-bonds, explicitly selected commissions,
  nonzero 32-byte metadata commitments, set policies, totals, and snapshot
  commitments;
- the constitution and emergency-policy hashes;
- an explicitly unfinalized initial checkpoint state with no signatures or PoW
  satisfaction.

Collections are sorted by wire asset and address identities before hashing.
`TridentBlockZeroState::verify` checks supply conservation, disjoint
allocation-backed vesting and validator self-bonds, independent validator-set
identities, fingerprint consistency, complete one-to-one manifest-field
coverage, and the initial finality state. `verified_borsh_payload` then performs
an exact Borsh round trip and rechecks both roots.

## Candidate storage envelope

Explicit Meta keys under `meta/trident_block_zero/` hold a versioned Borsh
envelope (`TRIDENT_BLOCK_ZERO_STORAGE_VERSION = 4`, independent of live
`SCHEMA_VERSION`). The envelope preserves the complete manifest, canonical
payload, manifest root, composed state root, commitment, commitment hash,
artifact identity, consensus policy hash, network fingerprint, chain ID, and
bound datadir identity.

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

## Offline live-state plan

`TridentBlockZeroState::plan_live_state` produces
`TridentLiveStatePlan` only after verifying the manifest and an independently
supplied `TridentHeader`. It derives:

- a versioned Block 0 body containing the canonical TLT issuance outputs and
  deterministic `issuance_id || output_index_le` outpoints;
- allocation provenance and exact TLT UTXOs, plus asset-scoped OVL/DRC account
  records whose liquid balances exclude validator self-bonds;
- complete supply buckets and derived maximum/issued/reserve keys;
- treasury balances and lossless artifact-selected control records;
- explicit vesting records that bind either a TLT outpoint or an OVL/DRC
  account and provide deterministic cliff/linear lock arithmetic;
- validator policies and records, epoch-zero snapshots, zero reward pools,
  staking reserves, an accepted Block 0 issuance record, and an explicitly
  unfinalized initial finality record;
- explicit empty DRC-payment and community roots, versioned body/header
  records, and genesis/tip/index metadata for the eventual loader.

Each manifest leaf has exactly one primary `(column family, key, value)`
mapping. Runtime balances and indexes are deterministic derivatives and cannot
replace or silently default a primary record. Keys are sorted and duplicate
keys or source mappings fail closed. Per-asset conservation proves
`liquid + validator bonds = genesis allocation`,
`genesis allocation + treasury = issued`, and
`issued + staking reserve + unissued = maximum`.

The composed root domain-separates identity, allocation, UTXO, each account
asset, supply, treasuries/controls, vesting, each staking set, acceptance,
initial finality, DRC payments, and community state. Planning writes the exact
record set only to a copy-on-write overlay, rereads every byte, recomputes every
component, and confirms equality with both commitment and header. The base
store is snapshotted before and after and must remain byte-for-byte unchanged.

## Atomic materialization consumer

`TridentLiveStatePlan::commit_atomically` is the only durable consumer of the
plan. For a fresh store it combines all canonical UTXO, account, supply,
treasury/control, vesting, staking, epoch, reward, finality, acceptance,
header, body, envelope, and datadir-identity records into one `WriteBatch`.
Before that one durable write, the complete batch is applied to a COW overlay
and every component/body/header/manifest root and identity is recomputed from
the staged bytes.

After the write, `reopen_verified_trident_live_state` independently rederives
the plan from the verified manifest and header, requires an exact store
snapshot, rereads the complete Block 0 envelope and datadir identity, and
recomputes every root. A write error or any missing, additional, malformed,
partial, tampered, or mismatched record returns an error and no readiness
value. Calling the commit API against the exact already-committed snapshot is
idempotent and performs no write; no other existing state is overwritten.

Only that durable verifier can construct `TridentLiveStateReadiness`. The
type's fields and constructor are sealed, it is not serializable, and it
proves only this storage prerequisite. It has no operation that starts
consensus, mining, P2P, or RPC.

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

The abstraction still does not construct the legacy [`agora_types::Block`],
or run inside `agora-node`. The storage prerequisite is complete, but a partial
boot path remains unsafe because:

1. The Trident header/body encoding and composed-root version remain
   offline-only. Consensus, PoW, acceptance, reorg, vesting-spend, treasury
   authorization, storage, P2P, and RPC gates must consume those exact versions
   before any node may boot them; the frozen v2 `BlockHeader`/`Block` path cannot
   be repurposed.
2. A future Trident node loader must independently verify the frozen artifact
   and concrete header, call `reopen_verified_trident_live_state`, and require
   its sealed capability before loading a libp2p key or binding RPC. No current
   startup path accepts that capability, so it cannot be bypassed accidentally.
3. The checked-in artifact is still unfrozen. The commit API consumes typed,
   already verified state and header inputs; it does not read
   `AGORA_GENESIS_FILE` or supply ceremony values.

Until one complete startup path consumes the capability and all activation
gates, `AGORA_GENESIS_FILE` remains the frozen v2 loader only.
