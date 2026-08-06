# Agora Constitution (Trident)

**Maturity:** Scaffold / Experimental. Extends the civic prototype in `agora-governance` toward hybrid L1 chambers. On-chain enforceability lands with Phase 5; until then Meta CF civic RPC remains an **administrative prototype**.

Existing text: [`CONSTITUTION.md`](CONSTITUTION.md), [`CIVIC_MODEL.md`](CIVIC_MODEL.md). This document defines the **Trident approval matrix**.

## Chambers

### OVL Technical Chamber

Execution changes, contract runtime, developer grants, OVL validator technical parameters, SDK/tooling priorities. Voting power: OVL stake / chamber rules.

### DRC Community Chamber

Community programs, merchant adoption, hubs, education, events, translation, community treasury priorities. Voting power: DRC stake / chamber rules.

### Ecclesia Citizens’ Chamber

Verified contribution or personhood credentials; one-person-one-vote where appropriate. Public-goods review, hub accreditation, grant oversight, appeals, conflict-of-interest review, constitutional vetoes, retrospective funding review.

### TLT miner signaling

PoW changes, block format readiness, security-critical release readiness, network upgrade signaling. **Must not** independently control community treasuries.

## Proposal classes (minimum)

| Class | Typical path |
| --- | --- |
| Technical execution upgrade | OVL Chamber → Ecclesia review → miner readiness → timelock |
| Payment-module upgrade | DRC Chamber → Ecclesia review → timelock |
| Consensus upgrade | OVL supermajority ∧ DRC supermajority ∧ miner readiness ∧ Ecclesia constitutional review → extended timelock |
| Community grant | DRC grant council/chamber → COI checks → milestone contract |
| Protocol development grant | OVL Chamber (+ Ecclesia as configured) → milestones |
| Hub accreditation | Ecclesia (+ DRC Community) |
| Treasury policy | Relevant chamber + Ecclesia |
| Constitutional amendment | All chambers + extended timelock |
| Emergency security action | Limited Security Council; narrow scope; automatic expiry; public disclosure; mandatory post-action ratification |

## Emergency powers — hard bans

Emergency action **must not**:

- Mint TLT, OVL, or DRC
- Confiscate ordinary balances
- Indefinitely suspend governance

## Treasuries

Three distinct protocol treasuries (no single private key control):

1. **TLT Security Treasury** — consensus engineering, audits, bounties, cryptography, FV, node infra, IR  
2. **OVL Builder Treasury** — SDKs, tooling, wallets, hackathons, contract audits, app teams  
3. **DRC Community Treasury** — hubs, education, merchants, events, translation, creators, support  

Each supports multisig/governance execution, public history, budget periods, spending limits, proposal-linked disbursements, milestone escrow, clawback where appropriate, COI records, independent review, public reports.
