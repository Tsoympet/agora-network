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
| `9` (current) | Canonical Hub, Passport, Grant, and Mission registry summary/records |

Meta key: `meta/schema_version` (`u32` LE). Missing key ⇒ treat as `1`.

## Rules

1. Never silently open a newer schema datadir with older code (or the reverse) on public networks.
2. Provide explicit `migrate` / `reindex` / `verify-invariants` commands before public testnet.
3. Trident public testnet prefers a **fresh genesis v3 datadir** over in-place upgrade from v2 economic state.
4. Lab `agora-layers` balances use the snapshot/claim path in [`OVL_DRC_TO_L1.md`](OVL_DRC_TO_L1.md) — not ad-hoc SQL/scripts.

## Meta key families (Trident)

- OVL/DRC accounts (`account/…`), acceptance (`acceptance/…`), per-asset supply
- Staking: `stake/val|del|unbond|epoch|snap/…`
- Finality: `finality/cert/<block_hash>`, `finality/tip_blue_score`
- DRC payments: `payment/drc/seen|invoice|outbox/…`
- Governance: `governance/consensus/policy`, `governance/treasury/<id>`
- Community: `community/v1/summary|hub|passport|grant|mission|issuer_nonce|active_issuer`

Atomic `WriteBatch` commit rules from PRs #76–#81 remain mandatory.
