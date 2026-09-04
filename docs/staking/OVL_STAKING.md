# OVL Staking

**Maturity:** Experimental (`agora-state-machine::staking` with `StakingParams::ovl_default`). Independent of DRC staking. RPCs: `agora_getValidatorSet` / `agora_getValidator` / `agora_getRewardPool` / **`agora_submitStakeTx`**.

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

**v1 (wired):** slash proceeds credit `stake/reward_pool/OVL` and distribute pro-rata on epoch advance (commission + self/delegator split).

**Reserve drip:** `stake/reserve_remaining/OVL` initialized from working genesis `StakingReserve.reserve_base_units` (10% of max; not ceremony-frozen); `drip_staking_reserve` / `epoch_reserve_drip` wired.

**Fee share:** Accepted OVL `AccountTransfer.fee` credits the reward pool via `credit_fee_share_to_reward_pool` — never divert TLT miner fees.
