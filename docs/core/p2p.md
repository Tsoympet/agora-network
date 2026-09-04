# P2P (`agora-p2p`)

Network communications for Agora **must** use `libp2p`.

## Stack

- Transport: TCP + Noise + Yamux
- Pub/sub: Gossipsub (strict validation, signed messages)
- Payloads: `borsh`-encoded `NetworkMessage`

## Mesh tuning & peer scoring

`NetworkConfig::gossip` (`GossipTuning`) defaults target sub-second BlockDAG tips:

| Knob | Default |
| --- | --- |
| heartbeat | 200ms |
| mesh_n / low / high | 6 / 4 / 12 |
| mesh_outbound_min | 2 |
| flood_publish | true |
| peer scoring | on |

Peer scoring uses gossipsub P1–P7 with Agora topic weights (blocks = 1.0, attestations = 0.75, txs = 0.5) for the configured network. Soft mesh-delivery thresholds avoid graylisting tiny local meshes. `agora-node` also sets application scores (`reward_peer` / `penalize_peer`) when RR/gossip blocks admit or reject.

## Topics (v1, network-scoped)

Topics and the getblock protocol are scoped by `NetworkConfig::network` (from `AGORA_NETWORK`):

| Name | Example (`testnet`) | Payload |
| --- | --- | --- |
| blocks | `agora/testnet/blocks/1` | `Block` / `BlockAnnounce` / `CompactBlock` / `GetBlock` |
| attestations | `agora/testnet/attestations/1` | `CheckpointAttestation` (Trident dual-PoS) |
| txs | `agora/testnet/txs/1` | `Transaction` / `AccountTransfer` / `StakeTx` / `OvlExecution` |
| getblock RR | `/agora/testnet/getblock/1` | CBOR `GetBlockRequest` / `GetBlockResponse` |

`dev` (default) uses `agora/dev/…`. Peers on different networks never share a gossip mesh even on the same underlay.

## Compact blocks / IBD

After a block is admitted locally, `agora-node` gossips:

1. `CompactBlock { header, short_ids }` for UTXO-only bodies, or a full `Block` when account/stake lanes are non-empty
2. `BlockAnnounce { hash }` — hash-only tip signal

Receivers try `reconstruct_compact_block` against the local mempool. On miss (or hash-only announce without a body), they request the body from the announcing peer over the network-scoped **`/agora/<network>/getblock/1`** protocol (libp2p request-response, CBOR). `PendingFetches` dedupes in-flight hashes. If request-response fails, the node falls back to gossip `GetBlock` / `Block`.

### Orphan pool (multi-hop IBD)

When a full body arrives but parents are unknown, `agora-node` **parks** it in an in-memory `OrphanPool` (TTL + max size) and issues GetBlock for each missing parent — without penalizing the peer. After a parent admits, `drain_orphans_after` re-tries waiting children (re-parking if other parents are still missing).

### Headers-first / locator IBD

On `PeerConnected`, the node builds a Bitcoin-style **block locator** along the virtual selected-parent spine and requests headers over **`/agora/<network>/getheaders/1`** (CBOR request-response). The peer returns an oldest→newest header batch after the common ancestor. The client validates parent links, then fetches missing bodies with GetBlock (oldest-first). Full batches re-issue GetHeaders until the peer is not ahead.

Serving peers walk durable Warm `header/*` records (Phase 34), so **pruned nodes** (`AGORA_ARCHIVAL=0`) can answer GetHeaders even after Hot bodies are dropped.

Orphan bodies persist under Warm `orphan/*` and are reloaded into the in-memory pool on restart (Phase 38).

Empty-tx templates reconstruct immediately (no mempool lookup).

| Test | Covers |
| --- | --- |
| `compact_block_ibd` | gossip announce / compact wire path |
| `getblock_request_response` | direct getblock roundtrip |
| `getheaders_request_response` | locator → headers RR |
| `topics` unit tests | `dev` vs `testnet` topic / protocol isolation |
| `ibd::orphan_pool_*` / `drain_orphans_*` | park / release / capacity / BFS drain |
| `admit::orphan_pool_recovers_out_of_order_child` | tip-before-parent → fetch path via pool |
| `admit::block_locator_and_headers_after_locator` | spine locator + header slice |
| `admit::multiblock_headers_first_catchup` | lagging peer syncs via batched GetHeaders + bodies |

## Mempool

The mempool reserves UTXO outpoints and one shared account nonce per `(asset, address)`. Account transfers, stake ops, and OVL execution therefore cannot race the same OVL/DRC nonce. Node admission runs the exact state-machine validator without committing before placing an operation in the pool.

`agora-node` runs `validate_mempool_tx` (live `cf_utxo` + mempool reserved set) under the same lock before admit on both RPC `agora_submitTransaction` and gossip `Transaction` messages. Missing, foreign, overspending, or already-reserved inputs are rejected at the edge. The implicit fee must be ≥ `AGORA_MIN_RELAY_FEE` (default 1); admission stores the fee for template ordering.

Mining templates pull UTXO transfers plus account/stake lanes and commit all lanes with `compute_body_root`. Coinbase value remains emission plus TLT transfer fees only; OVL/DRC account fees go to their reward pools during acceptance. On block admit, `evict_for_block` drops included operations and releases reservations.

## Runtime

`NetworkNode::build` constructs a swarm, subscribes to both topics, and emits `NetworkEvent`s (`Listening`, `PeerConnected`, `Message`, …).  
`NetworkNode::run` drives the swarm loop.

### Persistent identity

`agora-node` loads or creates `$AGORA_DATA/p2p/identity.key` (libp2p protobuf ed25519) before building the swarm. `NetworkConfig::identity` carries the keypair; when unset (tests), `build` still generates an ephemeral key. PeerId is therefore stable across restarts for the same datadir.

Integration test `two_node_gossip` dials two local nodes and exchanges a signed transaction.

## Connection limits

`libp2p::connection_limits` is wired from `NetworkConfig::max_peers` (`AGORA_MAX_PEERS`, default `64`):

| Limit | Value |
| --- | --- |
| max established (total / in / out) | `max_peers` |
| max established per peer | 1 |
| max pending in / out | `2 * max_peers` |

Exceeded dials/accepts surface as swarm connection errors (logged).

## DNS seeder

`infrastructure/dns-seeder` (`agora-dns-seeder`) is an HTTP phonebook:

| Endpoint | Purpose |
| --- | --- |
| `GET /peers` | JSON array of multiaddrs |
| `POST /peers` | Register a multiaddr |
| `GET /health` | Liveness |

Bind with `AGORA_SEEDER_BIND` (default `127.0.0.1:18080`). Preload via `AGORA_SEEDER_PEERS`.

### Node wiring

`agora-node` reads `AGORA_DNS_SEEDER` (host or full URL) into `NetworkConfig::dns_seeder_url`:

1. On boot, `fetch_seeder_peers` merges seeder results with `AGORA_BOOTSTRAP` (capped by `max_peers`) and dials
2. On first `Listening` event, register the dialable multiaddr (`0.0.0.0` → `127.0.0.1`)
3. Every `AGORA_SEEDER_REFRESH_SECS` (default `60`, `0` disables), `SeederBook` re-fetches peers, dials new multiaddrs, and re-registers

```bash
cargo run -p agora-dns-seeder
AGORA_DNS_SEEDER=http://127.0.0.1:18080 AGORA_SEEDER_REFRESH_SECS=60 cargo run -p agora-node
```

## Two-node local smoke

`scripts/local_testnet.sh` boots a seeder + two nodes with separate datadirs / listen / RPC ports. Each node persists its libp2p identity at `$AGORA_DATA/p2p/identity.key` (protobuf-encoded ed25519), so PeerIds survive restarts. Fresh wipe-two still generates new keys; discovery continues through the seeder (or stable `/p2p/<id>` bootstrap once you know the PeerId).

| Role | Data | Listen | RPC | Notes |
| --- | --- | --- | --- | --- |
| seeder | — | `127.0.0.1:18080` | — | `AGORA_SEEDER_BIND` |
| node-a | `data/agora-node-a` | `/ip4/127.0.0.1/tcp/16111` | `:8545` | fund enabled; registers dialable addr |
| node-b | `data/agora-node-b` | `/ip4/127.0.0.1/tcp/16112` | `:8546` | `AGORA_SEEDER_REFRESH_SECS=5` |

```bash
# Prebuild once so terminals don't race rocksdb compiles:
cargo build -p agora-dns-seeder -p agora-node -p agora-miner-sidecar

./scripts/local_testnet.sh wipe-two
./scripts/local_testnet.sh seeder          # terminal 0
./scripts/local_testnet.sh node-a          # terminal 1 — wait for "registered dialable addr"
./scripts/local_testnet.sh node-b          # terminal 2 — both should log "peer connected"
./scripts/local_testnet.sh wait-peers      # optional readiness gate
./scripts/local_testnet.sh smoke-tx        # signed premine spend on A → pending on B
./scripts/local_testnet.sh smoke-ibd       # mine N blocks on A (default 3), wait for B tip converge
./scripts/local_testnet.sh smoke-ibd-catchup  # late-join: wait for B to catch A (no mine)
```

The script prefers `target/debug/<bin>` when present. `smoke-tx` needs `apps/shared` npm deps (auto-installs once).

**Smoke proof (`smoke-tx`):** BIP-39 premine wallet (`abandon…about` external(0)) signs a small transfer via the shared light-client, submits on node-a, then polls node-b until `agora_getTransaction` is `pending` (or the tx appears in `agora_getMempool`). Both nodes must share the same fresh genesis premine UTXO (`wipe-two` first). Do **not** use `agora_fundAddress` — it only mints locally.

**Smoke proof (`smoke-ibd`):** mines `AGORA_SMOKE_IBD_BLOCKS` RandomX blocks against node-a (default **3** via `AGORA_MINE_MAX_BLOCKS`), then polls until node-b’s tip set matches (CompactBlock / GetHeaders / `GetBlock` IBD).

**Late-join (`smoke-ibd-catchup`):** does not mine — waits for a freshly started / wiped node-b to catch node-a’s existing tips (headers-first catch-up). Mine with `smoke-ibd` first if both nodes are still at genesis.

**Unit proof:** `admit::tests::multiblock_headers_first_catchup` syncs six blocks through limit-2 GetHeaders batches + body admit and asserts matching tips / virtual tip.

## Follow-ons

- [x] Mempool UTXO pre-checks before gossip admission
- [x] Coinbase outputs in mining templates
- [x] Mempool transfers in mining templates + eviction on admit
- [x] Two-node local seeder + gossip/IBD runbook
- [x] Automated mined-block IBD smoke (`smoke-ibd`)
- [x] Multi-block IBD smoke + late-join catch-up (`AGORA_SMOKE_IBD_BLOCKS`, `smoke-ibd-catchup`)
- [x] Persistent libp2p identity (`$AGORA_DATA/p2p/identity.key`)
- [x] `agora_getMempool` pending snapshot RPC
- [x] Automated tx gossip smoke (`smoke-tx`)
