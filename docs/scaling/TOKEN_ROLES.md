# Token roles: Bitcoin / Ethereum / XRP mapping

Agora keeps **three native marks on three layers**. Product roles map as:

| Mark | Layer | Analog | Consensus | Meaning in Agora |
| --- | --- | --- | --- | --- |
| **TLT** | L1 | **Bitcoin** | **Pure PoW** (RandomX) | Scarce UTXO settlement money |
| **OVL** | L2 | **Ethereum** | **Hybrid** PoW mint + bonded sequencers | EVM gas + smart-contract money |
| **DRC** | L3 | **XRP** | **Hybrid** PoW mint + bonded attestors | Payments / path payments / bridge rail |

## Consensus model

### TLT — pure PoW (Bitcoin-class)
Unchanged L1 BlockDAG: RandomX miners, GHOSTDAG, UTXO.
Fee market: `agora_estimateFee` uses mempool median + congestion premium; full mempool
evicts lower-fee txs for higher-fee admissions.

### OVL — hybrid (PoW + PoS sequencers) ≈ Ethereum
- **PoW** (`sha256_leading_zero`) still mints OVL coinbase to miners.
- **Bonded sequencers** lock OVL (`bond_sequencer`). Once any sequencer is active:
  - `submit_batch` / `finalize_due` require a bonded sequencer (`submit_batch_as` / `finalize_due_as`).
- Empty sequencer set ⇒ permissionless (bootstrap / tests).
- Persistent `revm` account **+ contract storage** in the L2 state root.
- Ethereum-class RPC: `eth_chainId`, `eth_blockNumber`, `eth_getBalance`,
  `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt`, `eth_call`,
  `eth_sendRawTransaction` (legacy RLP + secp256k1 recovery; compact fallback).
- Durable checkpoint under `AGORA_LAYERS_DATA`.

### DRC — hybrid (PoW + PoS attestors) ≈ XRP
- **PoW** still mints DRC coinbase to miners.
- **Bonded attestors** lock DRC on the hub (`bond_attestor`). Once any attestor is active:
  - Payments stay `Paid` until quorum → `Finalized`.
  - `claim_mint` requires attestor quorum (`attest_message`).
- Empty attestor set ⇒ instant finality (bootstrap / tests).
- Default quorum: 2-of-3 of active attestors.
- Same-district **Payment** + destination tag + fee.
- Hub **path payment** with XRPL-class `deliverMin`.
- Destination-tag **registry** + payment index for exchange deposit routing.
- Intent settle uses real path payments; when attestors are bonded, intents enter
  `AwaitingFinality` until `finalize_intent` after quorum.

## OVL ≈ Ethereum (L2) — in-tree completeness

| Capability | Status |
| --- | --- |
| Account/value transfers | **done** |
| CREATE + bytecode | **done** |
| Contract storage in state root | **done** |
| `eth_call` / `eth_getCode` / `eth_getStorageAt` | **done** |
| L2 mempool (`eth_sendRawTransaction`) | **done** (legacy RLP + compact) |
| Bonded sequencer set | **done** |
| secp256k1 signed Ethereum txs / RLP | **done** (legacy + EIP-155) |
| Durable L2 checkpoint | **done** (`AGORA_LAYERS_DATA`) |
| Full Ethereum MPT state roots | deferred (SHA-256 digests today) |

## DRC ≈ XRP (L3) — in-tree completeness

| Capability | Status |
| --- | --- |
| Same-district Payment + fee | **done** |
| Destination tags + registry / index | **done** |
| Path payment + deliverMin | **done** |
| Attestor quorum finality | **done** |
| Intent balance-backed settle | **done** |
| Trust lines / issued currencies / DEX order books | out of scope (native DRC only) |

## TLT ≈ Bitcoin (L1) — in-tree completeness

| Capability | Status |
| --- | --- |
| UTXO + PoW + GHOSTDAG | **done** |
| Mempool fee ordering + eviction | **done** |
| Congestion-aware fee estimate | **done** |
| Headers-first IBD + durable orphans | **done** |
| Public seeds / mainnet freeze | **ops / human** |

## What we deliberately do *not* do

- Do **not** put OVL or DRC into L1 UTXO
- Do **not** move OVL/DRC to pure PoS (keeps mined issuance)
- Do **not** run a second full PoW security budget on L2/L3 for ordering
- Do **not** claim byte-for-byte Bitcoin / Ethereum / XRPL protocol parity
