# State Machine (`agora-state-machine`)

Applies consensus-ordered blocks to durable storage.

## Column families (5)

| CF | Name | Purpose |
| --- | --- | --- |
| Hot | `cf_hot` | Tips + recent headers for sub-second validation |
| Warm | `cf_warm` | Recent history for RPC / explorer |
| Archival | `cf_archival` | Long-term block payloads |
| Meta | `cf_meta` | Genesis hash, supply caps, tips set |
| UTXO | `cf_utxo` | Spendable outputs keyed by outpoint |

Logical `StateZone::{Hot,Warm,Archival}` map onto the first three CFs.

## Genesis

`GenesisBuilder` constructs Block 0 (premine coinbase), then `ignite` writes:

- genesis block into hot + archival
- `meta/genesis_hash`, `meta/max_supply`, `meta/premine`, `meta/tips`
- premine UTXO into `cf_utxo`

Default caps: max supply 100,000,000 AGORA; premine 10,000,000 AGORA.

## UTXO apply / revert

`apply_block(store, block, coinbase_reward)` mutates `cf_utxo` and returns a `UtxoJournal`:

| Tx kind | Rules |
| --- | --- |
| Coinbase (`inputs` empty) | At most one per block; outputs ≤ `coinbase_reward` |
| Transfer | secp256k1 verify; each input owned by signer; input value ≥ output value |

`revert_journal` restores spent outputs and deletes created ones (used if persistence fails after apply).

`validate_mempool_tx(store, tx, reserved)` is the read-only counterpart for mempool / gossip admission: rejects coinbase-shaped txs, missing or foreign inputs, reserved double-spends, and outputs that exceed input value — without mutating `cf_utxo`.

Outpoint keys are `tx_id || index_le` (36 bytes), same as genesis.

## Storage backends

- Default/dev: in-memory map (portable CI)
- `--features rocksdb`: durable RocksDB (requires C++ toolchain)

## Node wiring

`ChainState::admit_block` order: PoW verify → `apply_block` → persist block/tips → `Dag`/`Ghostdag`.
Coinbase budget uses `EmissionSchedule::reward_at_blue_score(estimate)` where estimate is `max(parent.blue_score)+1`.
