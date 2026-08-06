# Trident Test Plan

**Maturity:** Scaffold. CI must stay green on every PR; do not merge on red.

## Unit

Multi-asset transfers; asset isolation; supply caps; fee calculation; OVL/DRC staking; delegation; unbonding; slashing; validator-set rotation; quorum calculation; finality certificates; governance authorization; treasury spending; grant milestone release; passport attestation verification; duplicate/replay rejection.

## Consensus

- PoW-only blocks remain unfinalized  
- OVL quorum without DRC → unfinalized  
- DRC quorum without OVL → unfinalized  
- Both PoS without required PoW → unfinalized  
- Full three-part condition finalizes  
- Equivocating validator detected  
- Finalized checkpoint not reverted under normal rules  
- Deterministic arrival-order behavior  
- Partition and recovery  
- Validator-set epoch transition  
- Reorg before finality; reject reorg beyond finality  

## Transaction acceptance

Fee-paying duplicate/conflicting siblings; exact duplicates; invalid missing inputs; duplicate inputs; cross-asset spends; reorg mempool resurrection; acceptance-aware explorer status; asset-aware fee attribution. Full structural validation even when conflict-lost.

## Persistence

Crash during block / finality / stake / treasury / grant milestone commit; restart recovery; snapshot export/import; migration repeatability; supply invariant after restart.

## Integration

Miner proposes; OVL+DRC attest; checkpoint finalizes; wallet sends three assets; OVL gas spend; DRC merchant payment; governance after timelock; grant milestone payment; multi-node convergence.

## CI gates (target)

Formatting; full workspace tests; clippy with warnings denied; RandomX-enabled node build/tests; RocksDB persistence tests; TypeScript tests; wallet builds; explorer build; Docker multi-node smoke; dependency/license audit (`cargo-deny`).

## Per-phase minimum

See [`../architecture/TRIDENT_PHASE0_AUDIT.md`](../architecture/TRIDENT_PHASE0_AUDIT.md) §10.
