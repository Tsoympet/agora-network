# Dual-PoS Finality (`agora-consensus` + `agora-state-machine` + `agora-node`)

**Maturity:** Experimental (admit + RPC + attestation gossip + state root + signed stake ops).

Trident finality is additive on top of TLT RandomX + GHOSTDAG. A checkpoint is irreversible under normal rules only when **all** of the following hold:

1. PoW work / depth threshold met  
2. ≥ ⅔ active **OVL** voting stake attested  
3. ≥ ⅔ active **DRC** voting stake attested  

Empty active sets never satisfy quorum (no admin bypass). Stake is never price-combined across sets.

## State root

`compose_trident_state_root` (domain `agora-trident-state-root-v1`) commits:

UTXO set ∥ OVL accounts ∥ DRC accounts ∥ OVL stake snap ∥ DRC stake snap ∥ tip acceptance ∥ finalized tip ∥ gov/treasury placeholder (`Hash::ZERO` until Phase 5)

Checkpoint bodies bind this root (no longer provisional zero).

## Types (`agora-types`)

| Type | Role |
| --- | --- |
| `CheckpointBody` | Domain-separated signing payload |
| `CheckpointAttestation` | secp256k1 attestation by OVL/DRC validator |
| `FinalityCertificate` / `CheckpointState` | Aggregated finality lifecycle |
| `SignedStakeTx` / `StakeOpKind` | Network-bound bond/delegate/unbond/withdraw |

## Node admit

- Reorg-beyond-finality guard before UTXO mutation  
- PoW certificate note on virtual tip  
- `admit_attestation` with equivocation handling  

## P2P / RPC

- Topic `agora/<scope>/attestations/1`  
- `agora_getFinality` / `agora_getFinalizedTip` / `agora_submitAttestation`  
- `agora_getValidatorSet` / `agora_getValidator` / `agora_getRewardPool`  
- **`agora_submitStakeTx`** — secp256k1-signed stake ops enter the mempool, gossip, and template stake lane

## Rewards

| Source | Status |
| --- | --- |
| Slash proceeds → reward pool → epoch distribute | Wired |
| Staking reserve drip (`stake/reserve_remaining`) | Working testnet defaults (10% of max); epoch drip wired |
| `credit_fee_share_to_reward_pool` | Called for Accepted OVL/DRC account-transfer fees in block apply |

Never from TLT PoW mint.

## Still deferred

- Versioned compact short IDs for account + stake lanes (current non-empty lane blocks use full-body gossip)  
- Ceremony-frozen reserve economics (replace working defaults)  
- Full OVL execution / DRC payment modules beyond account-transfer fee share  
- Gov/treasury roots in state root (Phase 5)  
- Validator signing daemon  

See [`../consensus/HYBRID_POW_DUAL_POS.md`](../consensus/HYBRID_POW_DUAL_POS.md).
