# State Machine (`agora-state-machine`)

Applies consensus-ordered blocks to durable storage.

## Column families (5)

| CF | Name | Purpose |
| --- | --- | --- |
| Hot | `cf_hot` | Recent block bodies for tip validation (pruned by `AGORA_HOT_WINDOW`) |
| Warm | `cf_warm` | Tx index (`tx/`), multi-lane revert journals (`utxo_diff/`), acceptance, durable headers (`header/`) — not pruned |
| Archival | `cf_archival` | Long-term block payloads (optional via `AGORA_ARCHIVAL`) |
| Meta | `cf_meta` | Genesis/policy, account modules, DA indexes/nonces, supply, tips — never pruned |
| UTXO | `cf_utxo` | Spendable outputs keyed by outpoint — never pruned |

Logical `StateZone::{Hot,Warm,Archival}` map onto the first three CFs.

### Retention (`agora-node`)

| Env | Default | Meaning |
| --- | --- | --- |
| `AGORA_ARCHIVAL` | `1` | When `0`/`false`, skip writing block bodies to `cf_archival` (pruned node) |
| `AGORA_HOT_WINDOW` | `64` | Tip-distance of bodies kept in `cf_hot` (`0` = unlimited). Older Hot bodies are deleted after admit (only if Archival still holds a copy when archival mode is on). **Headers** stay in Warm `header/*` so pruned nodes can still serve GetHeaders and rebuild the DAG. |

## Genesis

`GenesisBuilder` constructs Block 0 (premine coinbase), then `ignite` writes:

- genesis block into hot + archival
- `meta/genesis_hash`, `meta/max_supply`, `meta/premine`, `meta/tips`, `meta/virtual_tip`
- premine UTXO into `cf_utxo` (virtual-chain baseline; no per-block journal)

All of these mutations, including headers, transaction indexes, Trident supply
defaults, canonical governance/community defaults, tips, and the UTXO, are
prepared before storage and committed in one `WriteBatch`. Checked ignition
compares a fresh Block 0 identity before that commit and refuses malformed
genesis metadata or state-bearing datadirs with no genesis identity.

Default caps: max supply 100,000,000 **TLT**; premine 10,000,000 **TLT** (8 decimals).

Canonical networks live in `ChainParams` / `NetworkId` (`dev` | `testnet` | `mainnet`).
Each network also locks consensus policy: `daa` (`DaaConfig`), `ghostdag_k`, and `pow_algorithm` (plus genesis `bits`).
Genesis artifacts (v2) additionally freeze wallet HRP / provisional SLIP-0044 coin type and the TLT/DRC/OVL mark registry.
Testnet freezes Block 0 — see [`docs/genesis/`](../genesis/README.md). `load_or_ignite_checked`
rejects a datadir whose `meta/genesis_hash` ≠ the expected network hash.

For Trident v3, `TridentGenesisArtifact::to_runtime_policy` is the single
offline conversion into typed DAA, GHOSTDAG, emission, monetary, staking,
finality, policy-hash, and fingerprint values. It first applies the
freeze-readiness gate and therefore rejects placeholders, missing policy
values, and stale artifact hashes. A freeze-ready artifact can also produce a
versioned Block 0 manifest and a lossless Meta envelope
(`meta/trident_block_zero/*`, storage version 3, independent of
`SCHEMA_VERSION`). A versioned `meta/trident_datadir_identity/*` Borsh record
binds its chain ID, network fingerprint, artifact and policy identities, Block
0 commitment, committed state root, header identity, and optional concrete
header hash. Both records are staged, overlay-verified, and committed in the
same atomic batch. Checked reopen requires canonical byte-for-byte equality
between the independent identity record, the copy inside the envelope, and any
expected identity supplied by a future Trident startup.

Epoch-zero validators include ceremony-selected commission and nonzero 32-byte
metadata commitments. `BlockZeroValidatorSet::to_runtime_validator_entries`
checks exact policy equality, preserves the secp256k1 consensus/withdrawal
identities, and derives the existing asset-scoped `stake/val/` key plus
canonical `ValidatorRecord` bytes. This is a pure conversion surface; it does
not materialize live staking state.

`GenesisBuilder` still does not seed live balances from this candidate. Its
legacy load paths now reject every complete or partial Trident marker before
examining or creating v2 genesis state, including datadirs that also contain a
valid v2 genesis hash. The node does not consume the candidate until the
artifact's UTXO/account/treasury/vesting/validator/finality records can be
materialized atomically and their recomputed live root equals the committed
header state root. Frozen v2 loading and identities remain unchanged.

## Virtual UTXO (Phase 28)

Live `cf_utxo` follows **blues** of `Ghostdag::order_past(virtual_tip)`, not every DAG tip.

| Concept | Storage |
| --- | --- |
| Selected tip | `meta/virtual_tip` (max tip `blue_score`, then hash) |
| Per-block diff | Warm key `utxo_diff/` ‖ block_hash → borsh(`UtxoJournal`) |

Admission order: PoW → persist body/tips → DAG/GHOSTDAG → reorg UTXO from old virtual → new virtual → retarget DAA.

Non-selected parallel tips are stored but do not spend until they become blue in the virtual past. Switching virtual tip unapplies/applies journals along the common-prefix of the two blue orders.

**Migration:** datadirs from before Phase 28 applied every tip eagerly — wipe `AGORA_DATA` and resync.

### Issued supply (Phase 29)

`meta/issued_supply` starts at premine and increases by each applied blue’s coinbase **subsidy** (`coinbase_total − fees`). Reorgs subtract on unapply. Coinbase emission is clamped so issued never exceeds `meta/max_supply`.

## UTXO apply / revert

`apply_block(store, block, emission_reward)` mutates `cf_utxo` and returns a `UtxoJournal`. Transfer fees (`in − out`) are summed first; the coinbase budget is `emission_reward + fees`:

| Tx kind | Rules |
| --- | --- |
| Coinbase (`inputs` empty) | At most one per block; outputs ≤ emission + Σ fees |
| Transfer | secp256k1 verify; each input owned by signer; input value ≥ output value (surplus → miner) |

Helpers: `transfer_fee` / `sum_transfer_fees` for template assembly.

`revert_journal` restores spent outputs and deletes created ones (used if persistence fails after apply).

`validate_mempool_tx(store, tx, reserved)` is the read-only counterpart for mempool / gossip admission: rejects coinbase-shaped txs, missing or foreign inputs, reserved double-spends, and outputs that exceed input value — without mutating `cf_utxo`.

Outpoint keys are `tx_id || index_le` (36 bytes), same as genesis.

Transaction index (`cf_warm`): key `tx/` ‖ `tx_id`, value `block_id` ‖ `index` LE — written on genesis ignite and every `persist_block` for `agora_getTransaction`.

## Trident staking + finality store (Phase 3+)

Meta CF keys are additive; the authenticated DA lane raises the current
`SCHEMA_VERSION` to `10`:

- `stake/val|del|unbond|epoch|snap|reward_pool|reserve_remaining/…` — staking + slash/reward + reserve
- `finality/cert|idx|last_att/…`, `finality/tip_blue_score` — certificates, signer index, tip
- `compose_trident_state_root` — canonical multi-asset commitment for checkpoint bodies

Node admit enforces reorg-beyond-finality. Account, stake, OVL execution,
native DRC payments, and authenticated DA commitments enter versioned consensus
lanes. DRC payment metadata, DA records/replay cursors,
governance/treasuries, and the bounded Hub/Passport/Grant/Mission registry
commit in `agora-trident-state-root-v5`. Local unsigned civic/community RPC
state remains excluded. See [`community-registry.md`](community-registry.md),
[`data-availability.md`](data-availability.md),
[`ovl-execution.md`](ovl-execution.md),
[`drc-payments.md`](drc-payments.md), [`governance.md`](governance.md), and
[`finality.md`](finality.md).

## Authenticated DA state

- `da/v1/commitment/<source><sequence_be>` stores the first accepted signed
  authorization for that source sequence.
- `da/v1/operator_nonce/<address>` stores the next accepted replay nonce.
- Exact signed retries are `ExactDuplicate`; a different claimant for the same
  source sequence or a consumed/future nonce is `ConflictLost` under Virtual
  order.
- Invalid structure, secp256k1 signature, chain, genesis, or fingerprint fails
  the block before storage.
- `UtxoJournal.data_availability_meta_before` restores both key families in the
  same crash-safe reorg batches as the other lanes.

The live node leaves DA policy activation unset because no TLT byte/state fee
or sponsorship rule is frozen. This state consumer is therefore block-only and
fail-closed outside explicitly activated Trident contexts.

## Storage backends

- [`StateStore::open_in_memory`] — ephemeral map for unit tests / portable CI
- [`StateStore::open(path)`] — **RocksDB** when built with `--features rocksdb` (enabled by default on `agora-node`); otherwise falls back to in-memory (path ignored)

`agora-node` defaults to RocksDB under `AGORA_DATA` (default
`data/agora-node`). On boot it opens and verifies storage through
`prepare_legacy_datadir`; only after that succeeds may the callback load or
create `$AGORA_DATA/p2p/identity.key`. A Trident marker or genesis mismatch
therefore returns before P2P identity, swarm, seeder, or RPC setup. The node
then rebuilds the in-memory DAG/GHOSTDAG from durable tips.

## Node wiring

`ChainState::admit_block` proves full blue order on a copy-on-write overlay,
atomically persists DAG/body metadata with a pending-virtual marker, then
reconciles live state through durable apply/revert journals. Coinbase budget
uses `EmissionSchedule::reward_at_blue_score` at the simulated GHOSTDAG score.
