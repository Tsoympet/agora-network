# Datadir schema migration (Trident)

**Maturity:** Scaffold (Phase 1 constants). Migration commands land with Phase 2+.

## Versions

| `SCHEMA_VERSION` | Meaning |
| --- | --- |
| `1` (current default) | Pre-Trident L1 UTXO + virtual tip journals (genesis v2) |
| `2` (planned) | Trident multi-asset: OVL/DRC account CFs, per-asset supply, acceptance roots, finality certificates |

Meta key: `meta/schema_version` (`u32` LE). Missing key ⇒ treat as `1`.

## Rules

1. Never silently open a schema-2 datadir with schema-1 code (or the reverse) on public networks.
2. Provide explicit `migrate` / `reindex` / `verify-invariants` commands before claiming Phase 2 complete.
3. Trident public testnet prefers a **fresh genesis v3 datadir** over in-place upgrade from v2 economic state.
4. Lab `agora-layers` balances use the snapshot/claim path in [`OVL_DRC_TO_L1.md`](OVL_DRC_TO_L1.md) — not ad-hoc SQL/scripts.

## Planned CF additions (Phase 2+)

- OVL account + stake column families (or prefixed keys under Meta/Utxo-equivalent)
- DRC account + payment + stake families
- Acceptance bitmap / acceptance-root persistence
- Finality certificate tip
- Per-asset `meta/issued_supply/<asset>`

Atomic `WriteBatch` commit rules from PRs #76–#81 remain mandatory.
