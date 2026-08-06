# OVL Staking

**Maturity:** Scaffold. Independent of DRC staking.

## Purpose

OVL stake selects the **technical validator set** that signs Trident finality checkpoints and secures the execution/governance path.

## Features

- Validator registration (consensus public key, withdrawal address, metadata)
- Self-bond + delegation
- Activation, epochs, validator-set snapshots
- Unbonding period + withdrawal
- Commission and reward distribution
- Jailing, slashing, tombstoning (severe equivocation)
- Maximum validator count, minimum stake, delegation concentration controls
- Deterministic quorum: `3 * signed >= 2 * active` (integer)

## Conservative slash defaults (initial)

| Offense | Slash | Tombstone |
| --- | --- | --- |
| Double / conflicting checkpoint signature | 5% of validator bonded stake | Yes |
| Objectively invalid-state attestation | 1% | Jail ≥ 1 epoch |
| Extended downtime | 0% (jail only) | No |

These are defaults for testnets; production values require governance + audit. Do not invent extreme percentages without explanation.

## Rewards

From predetermined OVL staking emissions, OVL execution fee share, and slashing proceeds — never from TLT PoW mint.
