# Token roles (Trident L1)

> **Superseded non-goal.** Earlier revisions forbade putting OVL/DRC into L1 UTXO and kept them on L2/L3. **Agora Trident L1** moves all three marks into one canonical L1 state machine. See [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md).

| Mark | Locus (target) | Consensus role | Meaning in Agora |
| --- | --- | --- | --- |
| **TLT** | L1 UTXO | RandomX PoW block proposal / ordering | Scarce settlement money; base network fees; security treasury |
| **OVL** | L1 accounts | PoS validator set (finality) | Execution gas; builder economy; technical governance collateral |
| **DRC** | L1 accounts | PoS validator set (finality) | Payments / merchants / community economy; community governance collateral |

## Terminology

"Bitcoin-class", "Ethereum-class", and "XRP-class" describe **product roles**, not protocol equivalence.

- OVL is **not** Ethereum-equivalent (no claim of full MPT / EIP surface unless implemented and labeled honestly).
- DRC is **not** XRPL-equivalent (no trust lines / DEX by default) and is **not** a stablecoin unless a separately audited stabilizer exists.
- Prefer maturity levels (Scaffold → Mainnet ready) over “role-complete.”

## Issuance (target)

| Mark | After genesis |
| --- | --- |
| TLT | PoW subsidy only |
| OVL | Staking emissions + fee/slash policy — **never mined** |
| DRC | Staking/community emissions + fee/slash policy — **never mined** |

## Historical layer stack

`agora-ovolos-rollup`, `agora-bridge-sdk`, and `agora-layers` remain in-tree as **lab / reuse sources** while Trident modules land. They must not be described as the canonical money locus after Trident genesis v3. Migration: [`../migration/OVL_DRC_TO_L1.md`](../migration/OVL_DRC_TO_L1.md).

## Current code vs target

| Concern | Code on `main` today | Trident target |
| --- | --- | --- |
| TLT | L1 UTXO + RandomX | Unchanged locus; keep hardening |
| OVL | L2 ledger + revm lab | L1 native accounts + execution module |
| DRC | L3 district ledger lab | L1 native accounts + payment module |
| Finality | Pure PoW blues | PoW ∧ OVL ⅔ ∧ DRC ⅔ |
