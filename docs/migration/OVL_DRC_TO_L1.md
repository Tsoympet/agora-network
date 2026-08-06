# Migration: OVL/DRC Layer Model → Trident L1

**Maturity:** Scaffold.

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

## Testnet re-genesis

Trident testnet uses genesis **v3** and a new `chain_id` / network fingerprint. Peers on v2 TLT-only testnet do not automatically upgrade in place.
