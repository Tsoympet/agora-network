# Incident Response (Trident)

**Maturity:** Scaffold.

## Severity

| Level | Examples |
| --- | --- |
| SEV-1 | Consensus split, finality stall with economic impact, supply invariant break |
| SEV-2 | Partial partition, validator set bug, RPC auth failure on public endpoints |
| SEV-3 | Explorer/wallet UX defects, non-consensus doc errors |

## Immediate actions

1. Preserve logs, datadir, fingerprints, peer IDs — do not wipe evidence.  
2. Halt public mint/lab RPCs if somehow enabled.  
3. Prefer freeze + disclosure over silent “admin finalize.”  
4. Emergency Security Council actions: narrow scope, automatic expiry, public disclosure, mandatory ratification — **no mint, no confiscation, no indefinite governance suspend**.  
5. Run invariant verification and reindex tools before declaring recovery.

## Contacts / treasuries

Fund security response from the **TLT Security Treasury** once live. Until then, operators follow repo `SECURITY.md`.
