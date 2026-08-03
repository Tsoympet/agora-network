# State Machine (`agora-state-machine`)

Applies consensus-ordered blocks to durable storage.

## Column families (5)

| CF | Name | Purpose |
| --- | --- | --- |
| Hot | `cf_hot` | Recent block bodies for tip validation (pruned by `AGORA_HOT_WINDOW`) |
| Warm | `cf_warm` | Tx index (`tx/` ‖ tx_id) for RPC lookups — not pruned |
| Archival | `cf_archival` | Long-term block payloads (optional via `AGORA_ARCHIVAL`) |
| Meta | `cf_meta` | Genesis hash, supply caps, tips set — never pruned |
| UTXO | `cf_utxo` | Spendable outputs keyed by outpoint — never pruned |

Logical `StateZone::{Hot,Warm,Archival}` map onto the first three CFs.

### Retention (`agora-node`)

| Env | Default | Meaning |
| --- | --- | --- |
| `AGORA_ARCHIVAL` | `1` | When `0`/`false`, skip writing block bodies to `cf_archival` (pruned node) |
| `AGORA_HOT_WINDOW` | `64` | Tip-distance of bodies kept in `cf_hot` (`0` = unlimited). Older Hot bodies are deleted after admit (only if Archival still holds a copy when archival mode is on) |

## Genesis

`GenesisBuilder` constructs Block 0 (premine coinbase), then `ignite` writes:

- genesis block into hot + archival
- `meta/genesis_hash`, `meta/max_supply`, `meta/premine`, `meta/tips`
- premine UTXO into `cf_utxo`

Default caps: max supply 100,000,000 AGORA; premine 10,000,000 AGORA.

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

## Storage backends

- [`StateStore::open_in_memory`] — ephemeral map for unit tests / portable CI
- [`StateStore::open(path)`] — **RocksDB** when built with `--features rocksdb` (enabled by default on `agora-node`); otherwise falls back to in-memory (path ignored)

`agora-node` defaults to RocksDB under `AGORA_DATA` (default `data/agora-node`). On boot it uses `GenesisBuilder::load_or_ignite` and rebuilds the in-memory DAG/GHOSTDAG from durable tips.

## Node wiring

`ChainState::admit_block` order: PoW verify → `apply_block` → persist block/tips → `Dag`/`Ghostdag`.
Coinbase budget uses `EmissionSchedule::reward_at_blue_score(estimate)` where estimate is `max(parent.blue_score)+1`.
