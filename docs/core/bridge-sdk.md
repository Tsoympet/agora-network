# Bridge-in-a-Box (`agora-bridge-sdk`)

SDK for custom **District Chains** (gaming / privacy / general).

## Messages

| Direction | Hub action | District action |
| --- | --- | --- |
| `LockAndMint` | Lock sender balance | Mint to recipient |
| `BurnAndUnlock` | Unlock to recipient | Burn sender balance |

## API

- `DistrictConfig::{gaming,privacy}`
- `BridgeBox::register_district`
- `lock_and_mint` / `claim_mint` / `burn_and_unlock`

Consensus crates stay untouched — districts speak only through bridge messages.
