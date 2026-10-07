# Trident Block 0 commitment

**Maturity:** Experimental (live materializer + freeze-ready-only node path).
**Not** public-testnet ready. The checked-in v3 draft remains UNFROZEN.

`core/crates/state-machine/src/block_zero.rs` defines the versioned,
deterministic Borsh manifest. `block_zero_live.rs` materializes that manifest
into live UTXO, account, treasury, vesting, validator, and supply state.
Preparation is accepted only from a freeze-ready v3 artifact.

Manifest version 3 is a pre-freeze break. It retains the version 2 chain ID and
network-fingerprint binding and adds complete ceremony-selected validator
registration fields to `TridentBlockZeroState` and
`TridentBlockZeroCommitment`.

The manifest commits:

- chain ID and network fingerprint, together with the artifact and consensus-policy identities;
- every TLT, OVL, and DRC initial allocation and vesting lock;
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
`TridentBlockZeroState::verify` checks supply conservation, allocation-backed
vesting and validator self-bonds, independent validator-set identities, fingerprint
consistency, and the initial finality state. `verified_borsh_payload` then performs
an exact Borsh round trip and rechecks the state root.

## Candidate storage envelope

Explicit Meta keys under `meta/trident_block_zero/` hold a versioned Borsh
envelope (`TRIDENT_BLOCK_ZERO_STORAGE_VERSION = 3`, independent of live
`SCHEMA_VERSION`). The envelope preserves the complete manifest, canonical
payload, commitment, commitment hash, artifact identity, consensus policy hash,
network fingerprint, chain ID, and bound datadir identity.

`TridentDatadirIdentity` is a separately versioned Borsh record under
`meta/trident_datadir_identity/`. It binds the chain ID, network fingerprint,
artifact identity, consensus-policy hash, Block 0 commitment, committed
**manifest** state root, and the Trident header network identity. When a fully
specified live header is available, it also binds that header's canonical hash.

`TridentBlockZeroState::stage_verified_store_batch` places the envelope and
identity bytes in one `WriteBatch` without live balances. The live path uses
`stage_verified_live_envelope_batch` plus the live writes below.

The legacy `GenesisBuilder` load paths reject any complete or partial Trident
Block 0/datadir marker. `agora-node` v2 startup still refuses a Trident datadir
before loading `$AGORA_DATA/p2p/identity.key`.

## Live materialization

`materialize_trident_block_zero_live` / `load_or_materialize_trident_block_zero`
require `validate_freeze_ready`. They:

1. Build a concrete Block 0 body (`Block` + `BlockHeader`) whose coinbase
   outputs are the TLT allocations.
2. Write OVL/DRC liquid accounts as allocation minus vesting minus self-bond.
3. Write artifact treasuries, treasury controls, constitution hash, emergency
   policy hash, and vesting schedules into the governance store. The composed
   governance root domain is `agora-governance-treasury-root-v2`.
4. Write epoch-zero `ValidatorRecord` values under `stake/val/`.
5. Ignite per-asset supply counters from the artifact monetary policy.
6. Store the body, header, tx index, tips, and `GENESIS_HASH`.
7. Recompute `compose_trident_state_root` on a copy-on-write overlay.
8. Bind a `TridentHeader` whose `state_root` is that **live** composed root
   (the manifest hash stays on `TridentBlockZeroCommitment.state_root`).
9. Commit envelope + live writes only when the durable recomputed root equals
   the header. Nonce `0` is allowed only when `bits == 0`; otherwise nonce is
   ceremony-owned (`--nonce` / `AGORA_TRIDENT_BLOCK_ZERO_NONCE`).

`agora-node genesis trident materialize --file PATH --data PATH` and
`AGORA_TRIDENT_GENESIS_FILE` use this path. The public draft fails freeze-ready
before any write or P2P identity load. Subsequent GHOSTDAG, mining, RPC, and
IBD still use the existing `Block`/`BlockHeader` wire. `TridentHeader` is the
Block 0 identity commitment in Meta, not a replacement gossip header.
Public testnet does **not** collapse these into one hash: frozen v2 history
and compact gossip identify vertices by `Block::id()` (`BlockHeader`). The
Trident mesh is isolated by `trident_network_fingerprint` (artifact
identity + policy + protocol versions). `agora_getNodeInfo` reports
`genesis_hash` as the gossip id and, on a Trident datadir, also reports
`trident_header_hash`, `artifact_identity`, and `block_zero_commitment`.

Vested amounts are withheld from liquid balances. Beneficiary-signed
`VestingUnlock` claims (protocol v32) credit the remainder that is vested at
the including block's `header.timestamp_ms` (linear after `start`, nothing
before `cliff`, full at/after `end`). Unlocked progress is committed in
`agora-governance-treasury-root-v3` so unlock-then-spend-then-reunlock cannot
fork. The claim does not mint.

## Remaining public-testnet blockers

- The checked-in `trident.testnet.genesis.draft.json` is UNFROZEN. Ceremony
  must supply allocations, validator keys, hashes, timestamp, bits, and (if
  `bits != 0`) a Block 0 nonce. Tooling does not invent them.
- Frozen v2 `AGORA_GENESIS_FILE` / docker-compose still boot the TLT-only
  testnet, which has no OVL/DRC genesis validators, so dual-PoS never
  finalizes.
- Gossip, mining templates, and IBD continue to identify blocks by
  `BlockHeader` hash. That is required to keep frozen v2 history. The
  Trident header commitment is a Meta identity, not a GHOSTDAG parent.

Until a freeze-ready v3 artifact is published and multi-node boot+IBD+tx+
finality are demonstrated against it, status stays **INCOMPLETE**.
