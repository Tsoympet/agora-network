# State Machine (`agora-state-machine`)

Applies consensus-ordered blocks to durable storage.

## Column families (5)

| CF | Name | Purpose |
| --- | --- | --- |
| Hot | `cf_hot` | Recent block bodies for tip validation (pruned by `AGORA_HOT_WINDOW`) |
| Warm | `cf_warm` | Tx index (`tx/`), UTXO journals (`utxo_diff/`), durable headers (`header/`) — not pruned |
| Archival | `cf_archival` | Long-term block payloads (optional via `AGORA_ARCHIVAL`) |
| Meta | `cf_meta` | Genesis hash, supply caps, tips set — never pruned |
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

Default caps: max supply 100,000,000 **TLT**; premine 10,000,000 **TLT** (8 decimals).

Canonical networks live in `ChainParams` / `NetworkId` (`dev` | `testnet` | `mainnet`).
Each network also locks consensus policy: `daa` (`DaaConfig`), `ghostdag_k`, and `pow_algorithm` (plus genesis `bits`).
Genesis artifacts (v2) additionally freeze wallet HRP / provisional SLIP-0044 coin type and the TLT/DRC/OVL mark registry.
Testnet freezes Block 0 — see [`docs/genesis/`](../genesis/README.md). `load_or_ignite_checked`
rejects a datadir whose `meta/genesis_hash` ≠ the expected network hash.

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

Meta CF keys (additive; `SCHEMA_VERSION = 4`):

- `stake/val|del|unbond|epoch|snap|reward_pool|reserve_remaining/…` — staking + slash/reward + reserve
- `finality/cert|idx|last_att/…`, `finality/tip_blue_score` — certificates, signer index, tip
- `compose_trident_state_root` — canonical multi-asset commitment for checkpoint bodies

Node admit enforces reorg-beyond-finality; RPC/P2P admit attestations; `agora_submitStakeTx` applies signed stake ops. See [`finality.md`](finality.md).

## Storage backends

- [`StateStore::open_in_memory`] — ephemeral map for unit tests / portable CI
- [`StateStore::open(path)`] — **RocksDB** when built with `--features rocksdb` (enabled by default on `agora-node`); otherwise falls back to in-memory (path ignored)

`agora-node` defaults to RocksDB under `AGORA_DATA` (default `data/agora-node`). On boot it uses `GenesisBuilder::load_or_ignite` and rebuilds the in-memory DAG/GHOSTDAG from durable tips.

## Node wiring

`ChainState::admit_block` order: PoW verify → `apply_block` → persist block/tips → `Dag`/`Ghostdag`.
Coinbase budget uses `EmissionSchedule::reward_at_blue_score(estimate)` where estimate is `max(parent.blue_score)+1`.
