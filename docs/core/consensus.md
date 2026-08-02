# Consensus (`agora-consensus`)

GHOSTDAG ordering, difficulty adjustment, PoW verification, and emission.

## GHOSTDAG

Agora orders a BlockDAG rather than a single chain. Parallel blocks are retained; GHOSTDAG selects a blue set (parameter `k`) and induces a total order for transaction conflict resolution.

Phase 2 implements full recursive blue-set inheritance. The current `Ghostdag::order_tip` API freezes the call surface for state-machine and node wiring.

## PoW

| Algorithm | Target hardware | Integration |
| --- | --- | --- |
| RandomX | CPU | `miner-sidecar` |
| kHeavyHash | ASIC | `infrastructure/stratum-pool` |

Verification is trait-based (`PowVerifier`) so consensus never embeds mining loops.

## Emission

`EmissionSchedule` owns reward math (initial subsidy + halving interval). Callers must not hardcode rewards elsewhere.
