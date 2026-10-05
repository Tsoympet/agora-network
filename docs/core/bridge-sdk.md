# Bridge-in-a-Box (`agora-bridge-sdk`)

**Maturity:** Experimental historical lab; non-canonical under Trident.

Retained SDK for reproducing former **District Chain** payment flows (gaming /
privacy / general) and porting bounded typed semantics. It is not a deployed
DRC network, canonical monetary state, or public Trident API. Canonical DRC is
native Trident L1 contract-free account/payment state.

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

## DRC ledger + genesis

`DrcLedger` holds historical district/hub balances under the former
**Drachma L3 genesis** cap. In this lab only, DRC was modeled as L3 PoW money
(`sha256_leading_zero` blocks + coinbase). Trident DRC is never mined and does
not inherit those balances or issuance rules.

Both the historical and canonical DRC ledgers are closed typed state machines:
there is no VM, bytecode, deploy/call, Hook, script, or contract-facing API.
Bridge messages and payment routing metadata are data for fixed protocol
transitions, not executable input.

| Artifact | Path |
| --- | --- |
| Testnet (frozen) | [`docs/genesis/drachma.testnet.genesis.json`](../genesis/drachma.testnet.genesis.json) |
| Mainnet draft | [`docs/genesis/drachma.mainnet.genesis.draft.json`](../genesis/drachma.mainnet.genesis.draft.json) |

`DrachmaGenesis` / `BridgeBox::from_genesis` load historical caps, hub id,
districts, and premine. `agora-layers` reads `AGORA_DRC_GENESIS_FILE` (default:
embedded testnet) only inside the loopback lab.

## API

- `DistrictConfig::{gaming,privacy,general}`
- `BridgeBox::register_district`
- `credit_hub_lock` / `lock_and_mint` / `claim_mint` / `claim_mint_with_proof` / `burn_and_unlock`
