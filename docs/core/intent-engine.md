# Intent-Engine (`agora-intent-engine`)

L4 orchestration: users declare outcomes; solvers propose routes; settlement uses Bridge-in-a-Box and/or a constant-product AMM.

## Intent

```
give (district, amount) → want (district, min_receive) before deadline
```

## Solvers

| Solver | Role |
| --- | --- |
| `NaiveSolver` | Cross-district 1:1 via hub |
| `AmmSolver` | Same-district constant-product pool |
| `CompositeSolver` | AMM when same-district, else naive bridge |

## Settlement

`route_and_settle`:

- Marks intent `Routed` then `Settled`
- Bridge path: `credit_hub_lock` → `lock_and_mint` → `claim_mint`
- AMM path: applies swap on the shared `ConstantProductPool`
- `cancel` abandons open/routed intents
