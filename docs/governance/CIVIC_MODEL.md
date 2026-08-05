# Agora civic governance — model overview

> Full charter: [`CONSTITUTION.md`](./CONSTITUTION.md)  
> Crate docs: [`docs/core/governance.md`](../core/governance.md)

## Problem we closed

Phase 3 only shipped **vote-weight math** (√ + 5% whale cap). Missing were:

1. A **constitution** (higher law)
2. **Rank names** for elected officers
3. **Where / how** proposals are voted (chambers + lifecycle)

## Design (short)

Inspired by **Cosmos Hub** (deposit → vote → timelock → execute), **Polkadot
OpenGov** (separate tracks by risk), and classical **Athens** (Ecclesia, Boule,
Archons) — matching Agora’s Greek product language (TLT / OVL / DRC).

```text
                 ┌─────────────────────┐
                 │   Constitution v1   │  higher law + content hash
                 └──────────┬──────────┘
                            │
     ┌──────────────────────┼──────────────────────┐
     ▼                      ▼                      ▼
 Ecclesia               Boule              Archon Collegium
 (all TLT, √ votes)   (21 Bouleutai)     (3 Archons)
     │                      │                      │
     └──────────┬───────────┴──────────┬───────────┘
                ▼                      ▼
         Proposal kinds ──► primary chamber vote
                │
                ▼
     Deposit → Voting → Tally → Timelock → Execute
                │
                ▼
     Seat ranks / amend constitution / spend / params
```

## Ranks

| Code | Title | Seats |
| --- | --- | --- |
| `ArchonEponymous` | Archon Eponymous | 1 |
| `ArchonBasileus` | Archon Basileus | 1 |
| `ArchonPolemarch` | Archon Polemarch | 1 |
| `Bouleutes` | Bouleutes | 21 |
| `Tamias` | Tamias | 3 |

## Next wiring (node)

- Persist `GovernanceState` under RocksDB meta
- JSON-RPC: submit / deposit / vote / list proposals / offices
- Explorer + desktop: Ecclesia ballot UI

Until then the constitution and `agora-governance` engine are the **normative
spec** implementations.
