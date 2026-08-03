# Bridge-in-a-Box (`agora-bridge-sdk`)

SDK for custom **District Chains** (gaming / privacy / general).

## Messages

| Direction | Hub action | District action |
| --- | --- | --- |
| `LockAndMint` | Debit hub lock + burn hub DRC | Mint DRC on claim |
| `BurnAndUnlock` | Credit hub lock + mint hub DRC | Burn district DRC |

Deposit into the hub with `credit_hub_lock` before `lock_and_mint`.

## Light-client proofs

- `merkle_root` / `prove_inclusion` / `verify_inclusion` over message IDs
- `BridgeBox::claim_mint_with_proof` requires a valid inclusion proof against a trusted root

## Messaging transport

`MessageTransport` abstracts production handoff:

- `publish` / `poll` per district lane
- `commit_root` / `prove` / `verify_against_root`
- `InMemoryTransport` for local multi-district sims
- Optional `BridgeBox::with_transport` publishes on lock/burn

## DRC ledger

`DrcLedger` holds district/hub balances under the genesis DRC cap. Layered mark only — not an L1 UTXO asset.

## API

- `DistrictConfig::{gaming,privacy,general}`
- `BridgeBox::register_district`
- `credit_hub_lock` / `lock_and_mint` / `claim_mint` / `claim_mint_with_proof` / `burn_and_unlock`
