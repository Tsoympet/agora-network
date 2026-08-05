# State Machine (`agora-state-machine`)

Applies consensus-ordered **accepted** transactions to durable storage.

## Column families (5)

| CF | Name | Purpose |
| --- | --- | --- |
| Hot | `cf_hot` | Tips + recent headers for sub-second validation |
| Warm | `cf_warm` | Acceptance bitmaps, tx index, explorer history |
| Archival | `cf_archival` | Long-term block payloads |
| Meta | `cf_meta` | Genesis hash, supply caps, tips, **network fingerprint** |
| UTXO | `cf_utxo` | Spendable outputs keyed by outpoint |

Logical `StateZone::{Hot,Warm,Archival}` map onto the first three CFs.

## Network fingerprint / datadir binding

`meta/network_fingerprint` stores the full `NetworkFingerprint`. `GenesisBuilder::ignite` refuses to write into a datadir that already has a different (or any existing) fingerprint. Node reboot loads the fingerprint and calls `assert_datadir_fingerprint`.

## Genesis

`GenesisBuilder` constructs Block 0 (premine coinbase), then `ignite`:

1. Writes block + supply caps + tips + fingerprint
2. Runs `accept_blue_blocks` on genesis
3. `commit_acceptance` atomically persists the acceptance bitmap and UTXO journal

Default caps: max supply 100,000,000 AGORA; premine 10,000,000 AGORA.

## Acceptance + UTXO journals

`commit_acceptance` applies `UtxoJournalOp::{Create,Spend}` together with:

- `accept/bitmap/<block>` — `AcceptanceBitmap`
- `accept/summary/<block>` — fees / subsidy / coinbase reward
- `accept/tx/<txid>` — `AcceptedTxRecord` for confirmations

All ops go through a single `StateStore::write_batch`.

`tx_confirmation(store, tx_id, tip_blue_score)` returns acceptance-based confirmations for RPC/explorer.

## Storage backends

- Default/dev: in-memory map (portable CI)
- `--features rocksdb`: durable RocksDB (requires C++ toolchain; use `CXX=g++` when clang’s libstdc++ headers mismatch)
