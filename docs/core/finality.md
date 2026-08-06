# Dual-PoS Finality (`agora-consensus` + `agora-state-machine` + `agora-node`)

**Maturity:** Experimental (admit + RPC + attestation gossip wired; not public-testnet ready).

Trident finality is additive on top of TLT RandomX + GHOSTDAG. A checkpoint is irreversible under normal rules only when **all** of the following hold:

1. PoW work / depth threshold met  
2. ≥ ⅔ active **OVL** voting stake attested  
3. ≥ ⅔ active **DRC** voting stake attested  

Empty active sets never satisfy quorum (no admin bypass). Stake is never price-combined across sets.

## Types (`agora-types`)

| Type | Role |
| --- | --- |
| `CheckpointBody` | Domain-separated signing payload (`agora-trident-checkpoint-v1`) |
| `CheckpointAttestation` | secp256k1 signature over body by an OVL or DRC validator |
| `FinalityCertificate` | Aggregated PoW flag + signed/active stake + `CheckpointState` |
| `CheckpointState` | `Proposed` → `PoWAccepted` / `Awaiting*Quorum` → `Finalized` |

## Consensus logic (`agora-consensus`)

- `quorum::has_two_thirds_quorum` — integer `3 * signed >= 2 * active`
- `finality::evaluate_checkpoint_state` / `refresh_certificate` / `note_pow_progress` / `note_signed_stake`
- `finality::assert_reorg_allowed` — reject target blue score ≤ finalized tip
- `evidence::detect_double_checkpoint` + conservative `SlashPolicy` defaults

## Crypto (`agora-crypto`)

`sign_checkpoint_attestation` / `verify_checkpoint_attestation` — TLT set rejected; pubkey must match address.

## State (`agora-state-machine`)

| Module | Persistence |
| --- | --- |
| `staking` | Independent OVL/DRC validators, delegation, epochs, snapshots, unbonding, evidence apply, **reward pool** (`stake/reward_pool/<asset>`) |
| `finality_store` | `finality/cert/<block_hash>`, `finality/idx/<block_hash>`, `finality/last_att/…`, `finality/tip_blue_score` |

Slash proceeds credit the per-asset reward pool and are distributed pro-rata on `advance_epoch` (commission + self/delegator split). Never from TLT mint.

## Node admit (`agora-node`)

On `admit_block`:

1. After virtual-tip selection, `guard_reorg_vs_finality` rejects tip changes that abandon any blue at or below the finalized frontier  
2. After UTXO reorg, `note_pow_on_virtual_tip` updates the tip certificate’s PoW leg  

`admit_attestation` verifies secp256k1 + registered consensus key, detects equivocation, updates the attestation index / signed stake, and persists the certificate.

## P2P

- Topic: `agora/<scope>/attestations/1`  
- `NetworkMessage::CheckpointAttestation`  
- Peer score weight `0.75` (between txs and blocks)

## RPC

| Method | Purpose |
| --- | --- |
| `agora_getFinality` | Certificate + state for a block hash |
| `agora_getFinalizedTip` | Finalized blue score marker |
| `agora_submitAttestation` | Admit + re-gossip one attestation |
| `agora_getValidatorSet` | Snapshot for OVL/DRC (+ optional epoch) |
| `agora_getValidator` | One validator record |
| `agora_getRewardPool` | Slash/reward pool balance |

Mutative bond/delegate RPCs remain deferred (need signed L1 stake txs; avoid `fundAddress`-class footguns).

## Still deferred

- Real multi-asset state-root commitment (provisional `Hash::ZERO` in checkpoint bodies today)  
- Staking-reserve drip from genesis `reserve_base_units`  
- Fee-share rewards (needs OVL execution / DRC payment fee attribution)  
- Validator signing daemon  

See [`../consensus/HYBRID_POW_DUAL_POS.md`](../consensus/HYBRID_POW_DUAL_POS.md) and staking docs.
