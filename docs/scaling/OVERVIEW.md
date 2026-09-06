# Scaling / layer crates (historical lab)

**Maturity:** Experimental / single-node prototype (in-process). **Deprecated as the canonical locus of OVL/DRC money** under Agora Trident L1.

Canonical design: [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md).

| Crate | Former role | Trident reuse |
| --- | --- | --- |
| `agora-ovolos-rollup` | L2 OVL + revm | Execution semantics → L1 OVL module (delete unsigned compact / dual balances) |
| `agora-bridge-sdk` | L3 DRC payments | Payment semantics → L1 DRC module (signed attestations, atomic mutation order) |
| `agora-intent-engine` | L4 intents | Optional app-layer |
| `agora-layers-runtime` / `agora-layers` | In-process compose | Loopback-only lab harness; mint/credit RPCs are never public or canonical |

```
Users / Agents
      │
      ▼
Agora Trident L1 (canonical)
  TLT UTXO · OVL accounts/execution · DRC accounts/payments
  Finality: PoW ∧ OVL quorum ∧ DRC quorum
```

## Run locally (lab only)

```bash
cargo run -p agora-layers
# JSON-RPC default: 127.0.0.1:8555
```

Do not claim public multi-chain deployment. Prefer genesis-native Trident balances over migrating lab ledgers ([`../migration/OVL_DRC_TO_L1.md`](../migration/OVL_DRC_TO_L1.md)).

The audited prerequisite for any future provenance-only L1 data commitment is
[`../core/data-availability.md`](../core/data-availability.md). There is
currently no live DA transaction or public district endpoint.
