# Transaction Acceptance (`agora-consensus::acceptance`)

The **transaction acceptance layer** is the single authority for which transactions become part of the UTXO set. Block color alone never implies acceptance, confirmation, fee credit, or mempool eviction.

## Pipeline

For each **blue** block in GHOSTDAG blue order:

1. **Validate every transaction fully**, independent of conflict outcome (structure, fingerprint-bound signature, **UTXO ownership** (`signer == utxo.address`), value conservation, coinbase shape).
2. **Require** `header.tx_root` commits to the body, and index-0 is always coinbase (hard-fail otherwise).
3. **Resolve conflicts by blue order** — first accepted exact `tx_id` wins; first accepted spend of an outpoint wins. Later duplicates / conflicts are rejected even when structurally valid.
4. **Emit a deterministic `AcceptanceBitmap`** aligned to `block.transactions` indices.
5. **Sum fees only from accepted non-coinbase transactions**.
6. **Accept coinbase iff** `sum(outputs) == subsidy + accepted_fees`, where subsidy is derived from `NetworkFingerprint` (premine for genesis, emission otherwise).

## Outputs

| Type | Role |
| --- | --- |
| `AcceptanceBitmap` | Packed accepted/rejected bits per tx index |
| `BlockAcceptance` | Bitmap + per-tx outcomes + fee/reward totals |
| `AcceptanceResult` | Per-block results + `UtxoJournalOp` list |
| `TxConfirmation` | RPC/explorer confirmations from acceptance depth |

## Persistence

`agora-state-machine::commit_acceptance` writes bitmaps, per-tx index records, and UTXO journal ops in **one atomic `write_batch`**.

## Consumers

| Consumer | Rule |
| --- | --- |
| RPC / explorer | `agora_getBlockAcceptance`, `agora_getTxConfirmation` — never “blue ⇒ confirmed” |
| Mempool | `Mempool::evict_by_acceptance` drops accepted txs and conflicting spends |
| Coinbase / emission | Reward = `EmissionSchedule` subsidy + accepted fees only |
| Datadir / signatures / gossip | Bound to full `NetworkFingerprint` |

## Non-goals

- Red-block transaction acceptance (red blocks are not supplied to this layer)
- Transport (HTTP) for RPC — method views live in `agora-rpc`; node wiring is separate
