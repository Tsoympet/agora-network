# Token roles: Bitcoin / Ethereum / XRP mapping

Agora keeps **three native marks on three layers**. Product roles map as:

| Mark | Layer | Analog | Consensus | Meaning in Agora |
| --- | --- | --- | --- | --- |
| **TLT** | L1 | **Bitcoin** | **Pure PoW** (RandomX) | Scarce UTXO settlement money |
| **OVL** | L2 | **Ethereum** | **Hybrid** PoW mint + bonded sequencers | EVM gas + smart-contract money |
| **DRC** | L3 | **XRP** | **Hybrid** PoW mint + bonded attestors | Payments / path payments / bridge rail |

## Consensus model

### TLT — pure PoW
Unchanged L1 BlockDAG: RandomX miners, GHOSTDAG, UTXO.

### OVL — hybrid (PoW + PoS sequencers)
- **PoW** (`sha256_leading_zero`) still mints OVL coinbase to miners.
- **Bonded sequencers** lock OVL (`bond_sequencer`). Once any sequencer is active:
  - `submit_batch` / `finalize_due` require a bonded sequencer (`submit_batch_as` / `finalize_due_as`).
- Empty sequencer set ⇒ permissionless (bootstrap / tests).

### DRC — hybrid (PoW + PoS attestors)
- **PoW** still mints DRC coinbase to miners.
- **Bonded attestors** lock DRC on the hub (`bond_attestor`). Once any attestor is active:
  - Payments stay `Paid` until quorum → `Finalized`.
  - `claim_mint` requires attestor quorum (`attest_message`).
- Empty attestor set ⇒ instant finality (bootstrap / tests).
- Default quorum: 2-of-3 of active attestors.

## OVL ≈ Ethereum (L2)

- Persistent `revm` account/contract state + CREATE
- OVL gas ledger + PoW coinbase
- Bonded sequencer set for ordering / finalize
- `eth_*` subset on `agora-layers`

## DRC ≈ XRP (L3)

- Same-district **Payment** + destination tag + fee
- Hub **path payment**
- Bonded attestor quorum for payment / bridge finality
- Intent settle uses real path payments

## What we deliberately do *not* do

- Do **not** put OVL or DRC into L1 UTXO
- Do **not** move OVL/DRC to pure PoS (keeps mined issuance)
- Do **not** run a second full PoW security budget on L2/L3 for ordering
