# Intent-Engine (`agora-intent-engine`)

L4 orchestration: users declare outcomes; solvers propose routes; settlement can use Bridge-in-a-Box.

## Intent

```
give (district, amount) → want (district, min_receive) before deadline
```

## Solver interface

Implement `IntentSolver::solve`. The scaffold ships `NaiveSolver` (1:1 cross-district via hub).

## Settlement

`route_and_settle` validates the solution against `min_receive`, then performs a `lock_and_mint` on the registered districts.
