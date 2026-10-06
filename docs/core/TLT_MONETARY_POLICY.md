# TLT monetary policy

**Maturity:** Experimental working defaults already present in code. A genesis ceremony may revise them only by an explicit consensus upgrade. This document does not introduce Bitcoin's 21,000,000 cap or a new subsidy.

## Units

8 decimal places. One whole TLT is `100_000_000` base units (`Amount::DECIMALS`).

## Supply

| Parameter | Value | Source |
| --- | --- | --- |
| Maximum supply | 100,000,000 TLT (`10_000_000_000_000_000` base) | `TLT_MAX_SUPPLY_BASE`, `SupplyCaps` |
| Genesis premine | 10,000,000 TLT (10% of max) | `SupplyCaps::default` |
| Initial subsidy | 50 TLT (`5_000_000_000` base) | `EmissionSchedule::default` |
| Halving interval | 210,000 blue-score units | `EmissionSchedule::default` |
| Subsidy after 64 halvings | 0 | `reward_at_blue_score` |
| Coinbase maturity | 100 blue score | `COINBASE_MATURITY` |
| Decimals | 8 | `Amount` |

The 50 TLT subsidy and 210,000 interval are the existing Agora schedule. They use the same integers as Bitcoin's initial reward and halving interval, and they were already committed in this repository. The unit is GHOSTDAG blue score, not a linear block height. The cap is 100,000,000 TLT, not 21,000,000 BTC. Those existing values stay.

Premine is issued at genesis and is exempt from coinbase maturity. Later issuance is only the coinbase. Emission is clamped so `issued + subsidy <= max`. Fees move existing TLT to the miner. They are not new issuance and they are not burned.

## What is unspecified

- A ceremony-frozen mainnet subsidy, if it will differ from the working 50 TLT schedule.
- A fee split from the miner to the security treasury. The policy hook exists in `docs/assets/MONETARY_POLICY.md`. Live coinbase still takes the full TLT transfer fee.
- A TLT base fee for data-availability bytes. DA admission stays fail-closed until that rule is reviewed.
- Whether covenant transactions, once activated, change the fee or the maturity rule. They do not today.

## Invariants

```text
issued_supply(TLT) <= 100_000_000 TLT
sum(unspent TLT outputs) accounts for premine + coinbase subsidies - coins still immature only as a spend rule, not as a supply reduction
OVL and DRC supplies are independent
```

No PoS mint of TLT. No governance mint. No contract mint.
