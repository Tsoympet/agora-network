# Dual-PoS Finality (`agora-consensus` + `agora-state-machine`)

**Maturity:** Experimental (library gadget + staking modules; not yet admitted on live block path / public RPC).

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
| `staking` | Independent OVL/DRC validators, delegation, epochs, snapshots, unbonding, evidence apply |
| `finality_store` | `finality/cert/<block_hash>`, `finality/tip_blue_score` |

Meta key prefixes: `stake/val/`, `stake/del/`, `stake/unbond/`, `stake/epoch/`, `stake/snap/`, `finality/`.

## Not in this phase

- P2P gossip of attestations  
- RPC `getFinality` / staking endpoints  
- Automatic certificate build on every blue tip in `agora-node`  
- Reward distribution accounting  

See [`../consensus/HYBRID_POW_DUAL_POS.md`](../consensus/HYBRID_POW_DUAL_POS.md) and staking docs.
