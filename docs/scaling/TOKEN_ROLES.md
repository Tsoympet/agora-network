# Token roles: Bitcoin / Ethereum / XRP mapping

Agora keeps **three native PoW marks on three layers**. Product roles map as:

| Mark | Layer | Analog | Meaning in Agora |
| --- | --- | --- | --- |
| **TLT** | L1 | **Bitcoin** | Scarce UTXO settlement money (RandomX BlockDAG) |
| **OVL** | L2 | **Ethereum** | EVM gas + smart-contract execution money on Ovolos |
| **DRC** | L3 | **XRP** | Fast payments / path payments / bridge liquidity between districts |

## OVL ≈ Ethereum (L2)

Implemented toward ETH-class UX:

- Persistent `revm` account/contract state across batches (CREATE when `to=0x0`)
- OVL native gas ledger + PoW coinbase on L2
- Ethereum-shaped JSON-RPC on `agora-layers`: `eth_chainId`, `eth_blockNumber`, `eth_getBalance`, `eth_getTransactionCount`
- Optimistic rollup lifecycle (sequence → challenge → finalize)

Still not full mainnet Ethereum parity (no MPT roots, limited `eth_*`, no public sequencer network yet).

## DRC ≈ XRP (L3)

Implemented toward XRPL Payment–class UX:

- Same-district **Payment** (`pay`) with fee + **destination tag**
- **Path payment** via hub (`path_pay`: burn/unlock → lock/mint/claim) debiting real balances
- Bridge lock/mint + burn/unlock corridors between districts
- Intent settle uses path payments (no faucet mint on settle)
- AMM settles against the DRC account ledger

Still not full XRPL (no trust lines / issued IOUs / order-book DEX / UNL consensus).

## What we deliberately do *not* do

- Do **not** put OVL or DRC into L1 UTXO (keeps TLT Bitcoin-clean)
- Do **not** make OVL an L1 world computer — it is the rollup’s ETH
- Do **not** make DRC an L1 asset — it is the district payments rail
