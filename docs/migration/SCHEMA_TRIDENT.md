# Datadir schema migration (Trident)

**Maturity:** Experimental (schema constants + library persistence). Full migrate/reindex CLI still pending.

## Versions

| `SCHEMA_VERSION` | Meaning |
| --- | --- |
| `1` | Pre-Trident L1 UTXO + virtual tip journals (genesis v2) |
| `2` | Acceptance records (`acceptance/<hash>`), per-asset supply keys, OVL/DRC account records under Meta |
| `3` | Additive Meta keys for OVL/DRC staking (`stake/…`) and finality certificates (`finality/…`) |
| `4` | State-root composition helpers; `stake/reserve_remaining/…`; signed stake ops (`agora_submitStakeTx`) |
| `5` | Multi-lane block body (`account_transfers` / `stake_ops`) + acceptance lanes; working non-zero OVL/DRC staking reserves |
| `6` | Signed `ovl_executions` lane, body-root v3, and full multi-lane acceptance commitment |
| `7` | Native `drc_payments` lane, body-root v4, duplicate/invoice indexes, and payment outbox |
| `8` | Canonical governance authorization policy and three asset-isolated protocol treasuries |
| `9` | Canonical Hub, Passport, Grant, and Mission registry summary/records |
| `10` | Authenticated DA commitment/source-sequence index, operator replay cursor, acceptance, and revert snapshots |
| `11` | Root-committed exact-delivery DRC receipt index and reorg snapshots |
| `12` | Owner-authorized DRC recipient policy and payment-v3 destination-tag presence |
| `13` | Address-based DRC DepositAuth policy/preauthorization state, acceptance, and reorg snapshots |
| `14` | Signed DRC payment-v4 GHOSTDAG blue-score expiry metadata |
| `15` | DRC regular-key state and authorization journals |
| `16` | DRC weighted signer-list state and authorization journals |
| `17` | Detached DRC multisign-attachment block lane |
| `18` | DRC master-key-disable policy with no-lockout enforcement |
| `19` | Additive DRC ticket, escrow, check, payment-channel, trust-line, issued-control, receipt, and revert-journal key families |
| `20` | Per-asset lifetime-burn counters and accepted-only DRC fee destruction |
| `21` | Common DRC live-object/owner index, accepted-operation receipts, and transaction-to-operation lookup |
| `22` (current) | OVL `u64` amounts are the exact `wei / 10^10` quotient. The dev-gated OVL-EVM-v1 world is committed under `meta/ovl/evm/v1/world` when present. DRC objects stay on the schema-21 index |

Meta key: `meta/schema_version` (`u32` LE). Missing key ⇒ treat as `1`.

## Rules

1. Never silently open a newer schema datadir with older code (or the reverse) on public networks.
2. Provide explicit `migrate` / `reindex` / `verify-invariants` commands before public testnet.
3. Trident public testnet prefers a **fresh genesis v3 datadir** over in-place upgrade from v2 economic state.
4. Lab `agora-layers` balances use the snapshot/claim path in [`OVL_DRC_TO_L1.md`](OVL_DRC_TO_L1.md) — not ad-hoc SQL/scripts.

## Meta key families (Trident)

- OVL/DRC accounts (`account/…`), acceptance (`acceptance/…`), per-asset issued
  supply (`meta/issued_supply/<asset>`) and lifetime burned supply
  (`meta/burned_supply/<asset>`)
- Staking: `stake/val|del|unbond|epoch|snap/…`
- Finality: `finality/cert/<block_hash>`, `finality/tip_blue_score`
- DRC payments: `payment/drc/seen|invoice|outbox|receipt/…`
- DRC policy/preauthorization:
  `policy/drc/account/…`, `policy/drc/deposit-preauth/<owner><source>`
- DRC authorization: regular keys, signer lists, tickets, and detached
  multisign-attachment commitments
- DRC settlement objects: escrow, checks, payment channels, issuer-scoped trust
  lines/liabilities, issued-asset controls, and point-query receipts
- Common DRC index: `ledger/drc/object/by-id|by-owner/…`,
  `ledger/drc/operation/by-id|by-tx/…`, and an exact version marker
- Governance: `governance/consensus/policy`, `governance/treasury/<id>`
- Community: `community/v1/summary|hub|passport|grant|mission|issuer_nonce|active_issuer`
- Data commitments: `da/v1/commitment/<source><sequence_be>`,
  `da/v1/operator_nonce/<address>`

Atomic `WriteBatch` commit rules from PRs #76–#81 remain mandatory.

Schema 21 does not reinterpret frozen historical block or transaction bytes.
The common descriptors and receipts are derived from existing typed DRC state,
accepted operation lanes, and detached multisign authorization. Their
state-transition, state-root, schema, and P2P fingerprint versions still gate
consensus compatibility. An older Experimental datadir needs the explicit
library migration, replay/reindex, or a fresh Trident datadir before public
activation; no operator migration CLI is claimed here.

The library-level schema 19-to-20 fee-burn migration is intentionally narrow:
it atomically creates zero burn counters and advances the schema, without
inferring historical fees from the mixed-provenance DRC reward pool. It refuses
other source versions and unexpected preexisting burn keys. Operational
activation/reindex tooling remains required before public testnet.

The library-level schema 20-to-21 migration requires canonical applied-blue
order and every non-genesis applied block's retained body, acceptance record,
and rollback journal. It reconstructs typed accepted-operation receipts,
rebuilds live descriptors and owner rows, and rewrites each historical journal
with exact common-index snapshots. The complete candidate state is verified on
a copy-on-write overlay before one atomic commit. Missing history, duplicate
order entries, partial common-index data, ambiguous transaction IDs, or
detached multisign blocks without the matching chain/genesis authorization
context fail closed. A pruned schema-20 node without that history must resync;
the migration does not infer receipts.
