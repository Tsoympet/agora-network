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

## Storage backends

- Default/dev: in-memory map (portable CI)
- `--features rocksdb`: durable RocksDB (requires C++ toolchain)

Block apply/revert atomicity beyond genesis is follow-on work.
