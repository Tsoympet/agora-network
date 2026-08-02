# State Machine (`agora-state-machine`)

Applies consensus-ordered blocks to durable storage.

## Triple-zone model

| Zone | Column family | Purpose |
| --- | --- | --- |
| Hot | `zone_hot` | Tips + balances needed for sub-second validation |
| Warm | `zone_warm` | Recent history for RPC / explorer |
| Archival | `zone_archival` | Long-term data, slower media OK |

Splitting zones avoids forcing tip validation through cold compaction paths.

## Storage

- Engine: `rocksdb` (enable crate feature `rocksdb`; requires a C++ toolchain)
- Default/dev builds use an in-memory map so `cargo test --workspace` stays portable
- Rocks open path creates missing column families
- Block apply/revert atomicity is Phase 3 work
