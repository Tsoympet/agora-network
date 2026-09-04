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

## Genesis freeze-readiness (offline)

The v3 verifier is **Scaffold** tooling. It validates a draft without blessing
it and separately provides a fail-closed freeze-readiness gate:

```bash
cargo run -p agora-node -- genesis trident verify \
  --file docs/genesis/trident.testnet.genesis.draft.json \
  --mode draft

! cargo run -p agora-node -- genesis trident verify \
  --file docs/genesis/trident.testnet.genesis.draft.json \
  --mode freeze-ready
```

The second command is expected to fail while the checked-in artifact contains
human-owned `UNFROZEN`, zero, provisional, allocation, and validator-set
placeholders. CI must retain both outcomes. A future ceremony candidate may
drop the `!` only after its recorded hashes and fingerprint recompute exactly.
This gate does not load v3 into a node and does not establish Public testnet
readiness.

## Integration

Miner proposes; OVL+DRC attest; checkpoint finalizes; wallet sends three assets; OVL gas spend; DRC merchant payment; governance after timelock; grant milestone payment; multi-node convergence.

### Automated multi-node crash smoke

`scripts/trident_multinode_crash.py` runs prebuilt node, RandomX miner, and DNS
seeder binaries with unique loopback ports and temporary datadirs. It:

1. waits for two nodes to become healthy and mutually connected;
2. verifies both nodes share genesis and converge on the initial tip set;
3. mines a block and verifies gossip convergence;
4. sends `SIGKILL` to the recorded node-B PID;
5. advances node A while B is offline;
6. restarts B with the same identity and datadir; and
7. verifies headers-first catch-up, tip convergence, and genesis stability.

Run it after building the three binaries:

```bash
cargo build -p agora-node -p agora-miner-sidecar -p agora-dns-seeder
python3 scripts/trident_multinode_crash.py --timeout 120
```

The default requires `agora_getFinality` and verifies that its response is
bound to the live tip. The temporary `--allow-pre-trident` flag is only for the
architecture-base CI branch, where the stacked finality RPC is not present.
Remove that flag when this harness is rebased onto the finality implementation.

The harness owns and terminates only PIDs it starts. A global deadline bounds
all readiness, mining, and recovery waits; failures print the tail of each
isolated process log.

## CI gates (target)

Formatting; full workspace tests; clippy with warnings denied; RandomX-enabled node build/tests; RocksDB persistence tests; TypeScript tests; wallet builds; explorer build; Docker multi-node smoke; dependency/license audit (`cargo-deny`).

## Per-phase minimum

See [`../architecture/TRIDENT_PHASE0_AUDIT.md`](../architecture/TRIDENT_PHASE0_AUDIT.md) §10.
