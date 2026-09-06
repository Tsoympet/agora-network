# Transaction Acceptance (`agora-state-machine`)

**Maturity:** Experimental (wired on Virtual apply + admit persistence).

The acceptance layer is the sole authority for which transactions mutate state, credit fees, confirm in RPC/explorer, and drive mempool eviction. **Block color alone never implies acceptance.**

## Pipeline (Virtual blue order)

1. Pre-select transferable txs (`selectable_transfers`) — preserves soft-skip / `pending_reserve` semantics from consensus hardening PRs #76–#81.
2. Fully validate auth even for soft-skipped transfers.
3. Mutate UTXO **only** for `Accepted` txs.
4. Emit `BlockAcceptanceRecord` with UTXO, account, stake, OVL execution, DRC
   payment, and authenticated DA statuses aligned to each lane.
5. Persist `acceptance/<block_hash>` in the same atomic `WriteBatch` as `utxo_diff/<block_hash>` and issued supply.

## Statuses

| Status | Meaning |
| --- | --- |
| `Accepted` | Mutates state; fees credit |
| `ExactDuplicate` | Same `tx_id` already accepted / coinbase outs exist |
| `ConflictLost` | Structurally valid; lost deterministic input conflict |
| `Invalid` | Structural/auth failure — currently **fails the block** (not soft) |

## Multi-asset

TLT remains UTXO. OVL/DRC use parallel account/stake lanes; OVL execution,
DRC payments, and provenance-only DA authorizations have dedicated lanes.
Apply mutates only `Accepted` operations. DA bodies use
`agora-block-body-v5`; their `(source, sequence)` key and per-operator replay
nonce follow the same Virtual first-winner rule. Exact duplicate means the
same signed authorization ID, not merely matching source data.

## RPC

`agora_getTransaction` includes TLT transaction acceptance. There is no DA
status RPC until pending/orphaned tracking can be reported without implying
hybrid finality.
