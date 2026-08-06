# Threat Model (Trident L1)

**Maturity:** Scaffold. Update as modules land.

## Assets to protect

- Integrity of the canonical state root and acceptance root  
- Finality certificates and validator-set commitments  
- Native supply of TLT, OVL, DRC  
- User keys and wallet vaults  
- Protocol treasuries  
- P2P availability and mesh isolation (fingerprint)

## Mandatory controls

- Full network-fingerprint signature binding  
- Domain-separated signatures per tx / governance / checkpoint type  
- Chain ID replay protection  
- Deterministic serialization (borsh)  
- Atomic database transactions; schema versioning; migration + reindex + invariant verify commands  
- State-root, acceptance-root, validator-set, finality-certificate commitments  
- Bounded P2P channels, peer quotas, persistent bans, rate limits  
- TLS or authenticated operator transport where applicable  
- Secure secret storage; no embedded production mnemonic  
- No fixed funded transaction caller  
- No silent fallback to an incompatible PoW algorithm on public networks  
- No floating-point consensus arithmetic  

## Notable threats

| Threat | Mitigation |
| --- | --- |
| Dual OVL balance inconsistency | Unify monetary state before production execution |
| Unsigned compact EVM path | Delete on L1; tests |
| Admin mint RPC on public nets | Feature-gate off; CI/config asserts |
| Single PoS set capture | Dual independent quorums required |
| Oracle-based fee conversion | Forbidden in consensus |
| Acceptance bypass via “blue ⇒ confirmed” | Explicit acceptance authority |
| Single-key treasury | Multisig / governance only |
| Side-branch merge regressing #76–#81 | Port acceptance concepts only |
| Equivocation without slash | Evidence + tombstone path |
| Emergency mint / confiscation | Constitution hard bans |

## Sybil / Passport

Passport attestations improve contribution signal but **do not** claim complete Sybil resistance unless a specific mechanism is implemented and audited.
