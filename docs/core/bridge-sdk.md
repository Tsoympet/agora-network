# Bridge-in-a-Box (`agora-bridge-sdk`)

SDK for custom **District Chains** (gaming / privacy / general).

## Messages

| Direction | Hub action | District action |
| --- | --- | --- |
| `LockAndMint` | Lock sender balance | Mint to recipient |
| `BurnAndUnlock` | Unlock to recipient | Burn sender balance |

## Light-client proofs

- `merkle_root` / `prove_inclusion` / `verify_inclusion` over message IDs
- `BridgeBox::claim_mint_with_proof` requires a valid inclusion proof against a trusted root

## Messaging transport

`MessageTransport` abstracts production handoff:

- `publish` / `poll` per district lane
- `commit_root` / `prove` / `verify_against_root`
- `InMemoryTransport` for local multi-district sims (libp2p adapter can implement the same trait)

## API

- `DistrictConfig::{gaming,privacy}`
- `BridgeBox::register_district`
- `lock_and_mint` / `claim_mint` / `claim_mint_with_proof` / `burn_and_unlock`
