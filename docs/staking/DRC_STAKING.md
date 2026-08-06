# DRC Staking

**Maturity:** Scaffold. Independent of OVL staking.

## Purpose

DRC stake selects the **community/payments validator set** that independently signs the same Trident finality checkpoints.

## Features

Same surface as OVL staking (registration, bond/delegate, epochs, unbonding, commission, rewards, jail/slash/tombstone, concentration controls, snapshots, deterministic quorum) with a **separate** validator set and parameters in genesis.

## Conservative slash defaults (initial)

| Offense | Slash | Tombstone |
| --- | --- | --- |
| Double / conflicting checkpoint signature | 5% of validator bonded stake | Yes |
| Objectively invalid-state attestation | 1% | Jail ≥ 1 epoch |
| Extended downtime | 0% (jail only) | No |

## Rewards

From predetermined DRC staking/community emissions, DRC payment fee share, and slashing proceeds — never from TLT PoW mint. DRC is not a stablecoin by virtue of staking.
