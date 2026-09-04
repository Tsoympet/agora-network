# Trident testnet operations and security readiness

**Maturity:** Scaffold.
**Launch decision:** **NO-GO** for a public Trident testnet.

This runbook defines the evidence required to change that decision. It does not
turn the current TLT-only testnet into Trident and must not be used to claim
public-testnet, audited-production, or mainnet readiness.

## Current deployment boundary

| Capability | Current architecture-base status | Public Trident gate |
| --- | --- | --- |
| Node boot | Frozen v2 TLT-only testnet | Node consumes a frozen v3 artifact |
| Trident genesis | Human-readable draft with `UNFROZEN` fields and empty OVL/DRC allocations and validator sets | Ceremony output is deterministic, reviewed, and load-tested |
| Assets | Trident types and offline policy validation exist; canonical node remains branch-dependent | TLT, OVL, and DRC state transitions are on the release commit |
| Finality | Design requires PoW ∧ OVL ≥2/3 ∧ DRC ≥2/3; architecture base has no finality RPC | Independent live validator sets and end-to-end finality evidence |
| Migration | Deterministic offline export and verification | Claims remain disabled unless a separate policy and transition are reviewed |
| Process recovery | Automated two-node hard-crash and same-datadir recovery smoke | Repeated release-candidate and soak evidence |
| Mainnet | Boot is intentionally refused before freeze | Out of scope for this runbook |

`docs/genesis/trident.testnet.genesis.draft.json` is ceremony input, not a
runnable node configuration. In particular, do not pass it to
`AGORA_GENESIS_FILE` until the node's v3 loader is integrated and the artifact
has no `UNFROZEN` or placeholder fields. The current Docker Compose file also
boots the frozen v2 TLT-only testnet.

## Security invariants

Release candidates must preserve all of these conditions:

1. TLT is the only mineable native asset and public networks use RandomX.
2. OVL and DRC validator quorums are calculated independently with integer
   stake arithmetic. Market prices never combine the sets.
3. A checkpoint finalizes only when its PoW threshold, OVL quorum, and DRC
   quorum all pass. No operator or administrator can waive a missing term.
4. PoW production may continue while either validator set is unavailable, but
   the resulting checkpoints remain explicitly **unfinalized**.
5. Every attestation binds the chain identity, policy and transition versions,
   checkpoint block, state root, height or blue score, and validator-set epoch.
6. TLT, OVL, and DRC supply conservation is checked independently. OVL and DRC
   cannot enter state through mining or a lab mint RPC.
7. Finalized history is not reverted by routine operator recovery. Any
   exceptional network reset requires a new public artifact and fingerprint.
8. RPC and seeder administration are authenticated on non-loopback interfaces;
   datadirs and validator keys are not shared between nodes.

The security model still assumes audited secp256k1 libraries, correct RandomX
verification, honest control of more than one third of each PoS set, and enough
honest PoW to satisfy the configured threshold. External review and adversarial
soak testing remain human release blockers.

## Genesis and fingerprint ceremony

The ceremony owner records participants, the candidate Git commit, toolchain,
input files, commands, and complete output digests. At least two independent
operators reproduce the result.

Required decisions:

- timestamp, PoW threshold policy, DAA parameters, and chain ID;
- every initial TLT, OVL, and DRC allocation;
- three treasury allocations and their multisig or governance controls;
- OVL and DRC staking reserves, validator limits, minimum self-bonds,
  unbonding periods, and genesis validator sets;
- constitution and emergency-policy content hashes;
- wallet HRP and the status of provisional SLIP-0044 coin type `8888`; and
- whether any experimental layer snapshot is excluded or committed.

Required checks:

```bash
cargo test -p agora-state-machine trident_genesis
cargo test -p agora-state-machine monetary
rg 'UNFROZEN|placeholder|scaffold-draft' \
  docs/genesis/trident.testnet.genesis.draft.json
```

The final `rg` command must return no matches against the proposed frozen
artifact. The release process must then:

1. serialize consensus fields deterministically and compute the genesis hash;
2. compute the network fingerprint from genesis, consensus policy, protocol,
   signing, and state-transition versions;
3. reproduce both values on a second clean machine;
4. boot two empty datadirs with the frozen artifact and
   `AGORA_EXPECTED_GENESIS`;
5. reject a one-byte policy or allocation mutation;
6. publish the artifact, hashes, commit, signatures, and reproduction log; and
7. require an explicit datadir reset from the v2 testnet.

Never edit a frozen artifact in place. Any consensus-relevant correction creates
a new chain ID and fingerprint.

## Release-candidate validation

Run repository gates on the exact commit and record their logs:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p agora-state-machine --features rocksdb rocksdb_persists_across_reopen
cargo test -p agora-consensus --test ghostdag_partition_fuzz
cargo build -p agora-node -p agora-miner-sidecar -p agora-dns-seeder
python3 scripts/trident_multinode_crash.py --timeout 120
```

On the architecture base only, the process smoke uses
`--allow-pre-trident`; that compatibility flag is forbidden for a Trident
release candidate. The release run must observe the finality RPC and prove it
is bound to the live tip. Follow `docs/testing/TRIDENT_TEST_PLAN.md` for the
remaining wallet, explorer, TypeScript, migration, load, and Docker gates.

The crash harness proves bounded orchestration, block gossip, a hard node-B
crash, offline advancement, persistent peer identity, headers-first catch-up,
and final convergence. It does not prove a long partition, validator compromise,
Byzantine quorum behavior, or production load.

## Secure operator baseline

Use one persistent datadir and one identity per node. Keep RPC private unless a
reverse proxy supplies TLS, authentication, request limits, and access logs.
Do not expose `AGORA_RPC_ALLOW_FUND` on a shared network.

Minimum node environment after v3 integration:

```bash
export AGORA_NETWORK=testnet
export AGORA_DATA=/var/lib/agora/node-a
export AGORA_GENESIS_FILE=/etc/agora/trident.testnet.genesis.json
export AGORA_EXPECTED_GENESIS=<published-genesis-hash>
export AGORA_LISTEN=/ip4/0.0.0.0/tcp/16111
export AGORA_DNS_SEEDER=https://seed.example.org
export AGORA_ARCHIVAL=1
export AGORA_RPC_BIND=127.0.0.1:8545
export AGORA_RPC_TOKEN=<secret-from-a-secret-manager>
```

Public testnet validators remain archival until finalized pruning points and
state snapshots are implemented. Ignore `AGORA_POW_ALGO`,
`AGORA_TEMPLATE_BITS`, and premine overrides on a frozen network; they are not
release controls.

Before starting:

- verify the binary and genesis checksums from separate trusted channels;
- ensure the data, identity, validator, and RPC-token files are owner-readable
  only;
- back up validator signing material using the approved key-management policy;
- configure clock synchronization, disk alerts, log rotation, and supervised
  restart limits; and
- deny inbound RPC and seeder registration except from intended networks.

Do not copy a live datadir or validator key to create another node. A duplicated
validator key can equivocate; a duplicated libp2p identity makes peer accounting
ambiguous.

## Monitoring and alerts

The following probes exist on the architecture base:

```bash
curl --fail --silent http://127.0.0.1:8545/health
curl --fail --silent http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getNodeInfo","params":[]}'
curl --fail --silent http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getDagTips","params":[]}'
```

After the corresponding RPCs land on the release branch, monitor finality,
validator-set epochs, reward pools, and protocol treasuries as consensus data.
Treat JSON-RPC `-32601` for any required Trident method as a deployment failure,
not a degraded healthy state.

Alert on:

- process or `/health` failure;
- zero peers, sustained peer-count collapse, or unexpected peer-ID change;
- genesis hash, chain ID, fingerprint, policy version, or transition-version
  mismatch;
- local tip lag or persistent divergence from independent reference nodes;
- checkpoint age above policy, including which of PoW, OVL, or DRC is missing;
- validator-set or voting-power changes around epoch boundaries;
- equivocation, jail, slash, or tombstone evidence;
- supply invariant, state-root, migration-root, or database recovery failure;
- disk exhaustion, repeated restart, RPC authentication failure, or unusual
  rate-limit volume; and
- use of lab funding, minting, compact unsigned execution, or migration claim
  activation on the shared network.

An unfinalized but advancing PoW tip is not “finality healthy.” Dashboards and
incident messages must distinguish availability, synchronization, and finality.

## Incident response

| Signal | Immediate action | Recovery rule |
| --- | --- | --- |
| Node crash | Preserve logs and datadir; restart the same binary and datadir | Verify genesis, peer ID, tip catch-up, state root, and finality before returning traffic |
| Database corruption | Stop the affected node and preserve evidence | Restore a verified snapshot or reindex according to a reviewed procedure; never copy another validator's keys |
| Tip divergence | Isolate public RPC and compare genesis, fingerprint, peers, and block acceptance | Do not delete data until consensus evidence identifies the bad branch |
| OVL or DRC quorum loss | Keep reporting new blocks as unfinalized and notify both operator sets | Never lower quorum or use an admin bypass |
| Equivocation evidence | Preserve signed messages and epoch snapshot | Apply only deterministic jail/slash rules implemented by the release |
| Suspected key compromise | Remove the endpoint from service and invoke the published key policy | Do not double-sign with a replacement key in the same epoch |
| Supply or state-root mismatch | Halt affected release rollout and preserve all artifacts | No manual balance edits; reproduce from genesis and escalate |
| Genesis or policy mismatch | Refuse peering and RPC traffic | Correct configuration or announce a new network; never silently join meshes |

For every incident, record UTC times, release commit, binary hash, genesis and
fingerprint, peer ID, validator epoch, last finalized checkpoint, current tips,
logs, actions, and decision owner. Publish a post-incident report for any
consensus or finality event.

## Migration safety

The preferred public-testnet path is genesis-native OVL and DRC. Historical
`agora-layers` checkpoints are experimental and confer no automatic claim.

If a migration is proposed, independently reproduce the snapshot:

```bash
cargo run -p agora-layers-runtime --bin agora-trident-migration -- \
  export --checkpoint-dir <frozen-checkpoint-dir> \
  --output <snapshot.json>
cargo run -p agora-layers-runtime --bin agora-trident-migration -- \
  verify --snapshot <snapshot.json> --require-ready
```

`--require-ready` must pass, all blockers must be resolved publicly, and supply
conservation and Merkle roots must be independently reproduced. The current
artifact keeps `claim_activation: false`; enabling claims requires a separate
reviewed state transition with replay protection. Never write exported balances
directly into a running datadir.

## Go/no-go record

The release owner may mark public Trident testnet **GO** only when every item is
linked to immutable evidence:

- [ ] Frozen v3 genesis has no placeholders and has two independent reproductions.
- [ ] Node loads v3 directly and rejects mismatched genesis and fingerprint.
- [ ] OVL and DRC genesis validator sets are non-empty and independently controlled.
- [ ] Three-asset supply, staking, transfer, execution, and payment invariants pass.
- [ ] PoW-only and every partial-quorum combination remain unfinalized.
- [ ] Full PoW ∧ OVL ∧ DRC finality passes across multiple nodes and epochs.
- [ ] Reorg before finality and rejection beyond finality pass.
- [ ] Crash, restart, partition recovery, arrival-order, and load/soak evidence pass.
- [ ] Required CI, wallet, explorer, Docker, dependency, and license gates pass.
- [ ] Migration is explicitly excluded or its separate claim review is complete.
- [ ] Public seeder, TLS RPC, monitoring, backups, and incident contacts are exercised.
- [ ] External security review findings are resolved or explicitly accepted.
- [ ] Release notes publish hashes, versions, endpoints, reset instructions, and limitations.

Current human blockers include genesis allocations, validator ownership and
parameters, treasury controls, policy hashes, reachable infrastructure, external
security review, and the final launch decision. Code or documentation cannot
resolve those choices autonomously.

## Related documents

- `docs/architecture/TRIDENT_L1.md`
- `docs/architecture/TRIDENT_PHASE0_AUDIT.md`
- `docs/testing/TRIDENT_TEST_PLAN.md`
- `docs/ops/PUBLIC_TESTNET.md`
- `docs/migration/OVL_DRC_TO_L1.md`
- `docs/core/p2p.md`
- `docs/core/rpc.md`
