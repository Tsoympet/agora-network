# DRC Staking

**Maturity:** Experimental (`agora-state-machine::staking` with `StakingParams::drc_default`). Independent of OVL staking. RPCs: `agora_getValidatorSet` / `agora_getValidator` / `agora_getRewardPool` / **`agora_submitStakeTx`**.

## Purpose

DRC stake selects the **community/payments validator set** that independently signs the same Trident finality checkpoints.

## Features

Same surface as OVL staking (registration, bond/delegate, epochs, unbonding,
commission, nonzero 32-byte metadata commitments, rewards,
jail/slash/tombstone, concentration controls, snapshots, deterministic quorum)
with a **separate** validator set and parameters in genesis. Genesis commission
must be explicitly selected within the set and 10,000 bps global bounds; the
checked Block 0 conversion preserves it, the metadata commitment, and both
secp256k1/withdrawal identities in the existing DRC staking key/value without
writing live state.

## Conservative slash defaults (initial)

| Offense | Slash | Tombstone |
| --- | --- | --- |
| Double / conflicting checkpoint signature | 5% of validator bonded stake | Yes |
| Objectively invalid-state attestation | 1% | Jail ≥ 1 epoch |
| Extended downtime | 0% (jail only) | No |

## Rewards

**v1 (wired):** slash proceeds credit `stake/reward_pool/DRC` and distribute pro-rata on epoch advance (commission + self/delegator split).

**Reserve drip / fee share:** same path as OVL (working non-zero `reserve_remaining`, Accepted DRC account fees → reward pool). Never from TLT PoW mint. DRC is not a stablecoin by virtue of staking.
