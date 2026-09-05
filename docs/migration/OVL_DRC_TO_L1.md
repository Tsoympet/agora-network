# Migration: OVL/DRC Layer Model → Trident L1

**Maturity:** Experimental (offline export and verification); claim activation remains Scaffold.

## Preferred path (default)

No value-bearing public multi-node OVL/DRC network must be preserved. **Launch Trident with OVL and DRC native from genesis** and retire layer-native issuance before mainnet.

- Keep `docs/genesis/ovolos.*.json` and `drachma.*.json` as **historical artifacts**.
- Mark `agora-layers` mint/credit RPCs as lab-only; disable on shared testnet/mainnet.
- Reuse payment/EVM **code**, not live balances, unless an operator explicitly opts into snapshot migration.

## If balances require migration

1. Select freeze heights  
2. Stop old issuance  
3. Export all balances  
4. Export stake and bonds  
5. Export locks and escrow  
6. Export pending bridge messages  
7. Export treasury positions  
8. Publish the complete snapshot  
9. Build a deterministic Merkle root  
10. Provide independent reproduction tooling  
11. Commit the root into the L1 upgrade / genesis  
12. Implement one-time claim transactions  
13. Prevent duplicate claims  
14. Publish a migration explorer view  
15. Audit supply conservation  
16. Disable old mint and settlement RPCs  
17. Define a final claim deadline or long-term recovery policy  

**Forbidden:** manually copying balances with undocumented scripts.

## Offline snapshot tool

`agora-trident-migration` reads the durable lab
`layers-checkpoint.json` without starting either network. It emits a
deterministic, content-addressed JSON artifact:

```bash
cargo run -p agora-layers-runtime --bin agora-trident-migration -- \
  export \
  --checkpoint-dir /path/to/agora-layers-data \
  --output /path/to/trident-migration-snapshot.json

cargo run -p agora-layers-runtime --bin agora-trident-migration -- \
  verify \
  --snapshot /path/to/trident-migration-snapshot.json
```

Use `verify --require-ready` in an operator freeze procedure. It exits non-zero
when the artifact is cryptographically intact but unresolved state still blocks
claim design.

The exporter:

- sorts and de-duplicates source records before commitment;
- aggregates DRC district balances by address while retaining the original
  district rows for reproduction;
- separates sequencer/attestor bonds from their reserved escrow balances so
  stake is not counted twice;
- validates historical OVL/DRC minted totals against their hard caps;
- records per-district DRC freeze tips and commits hashes of pending L2
  transactions;
- commits proposed OVL/DRC allocations into a domain-separated SHA-256 Merkle
  root;
- commits the complete audit body (allocations, district provenance, locks,
  messages, and quarantined EVM head state) into `snapshot_root`;
- reconciles minted, ledger, proposed-claim, and retired/burned totals; and
- reports bridge locks, pending messages or L2 transactions, escrow mismatches,
  and non-empty EVM head state as blockers.

The snapshot always contains `"claim_activation": false`. The tool does not
write an L1 datadir, mint assets, generate claim transactions, choose freeze
heights, or decide how historical EVM state and bridge locks should map into
Trident. Those actions require a separately reviewed policy and claim
transition.

### Conservation interpretation

Historical gas and fee paths can retire units without reducing every historical
`minted` counter. Therefore the audit binds:

```text
source_minted = source_ledger + retired_or_burned
source_ledger = proposed_claims
```

The second equality is mandatory for a ready artifact. Escrowed stake remains
part of the ledger and becomes a distinct proposed stake allocation, not an
additional balance.

## Testnet re-genesis

Trident testnet uses genesis **v3** and a new `chain_id` / network fingerprint. Peers on v2 TLT-only testnet do not automatically upgrade in place.
