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

Peer scoring uses gossipsub P1–P7 with Agora topic weights (`agora/blocks/1` = 1.0, `agora/txs/1` = 0.5). Soft mesh-delivery thresholds avoid graylisting tiny local meshes. `agora-node` also sets application scores (`reward_peer` / `penalize_peer`) when RR/gossip blocks admit or reject.

## Topics (v1)

| Topic | Payload |
| --- | --- |
| `agora/blocks/1` | `Block` / `BlockAnnounce` / `CompactBlock` / `GetBlock` |
| `agora/txs/1` | `Transaction` |

## Compact blocks / IBD

After a block is admitted locally, `agora-node` gossips:

1. `CompactBlock { header, short_ids }` — BIP152-style short ids (first 8 bytes of each `tx_id`)
2. `BlockAnnounce { hash }` — hash-only tip signal

Receivers try `reconstruct_compact_block` against the local mempool. On miss (or hash-only announce without a body), they request the body from the announcing peer over **`/agora/getblock/1`** (libp2p request-response, CBOR). `PendingFetches` dedupes in-flight hashes. If request-response fails, the node falls back to gossip `GetBlock` / `Block`.

Empty-tx templates reconstruct immediately (no mempool lookup).

| Test | Covers |
| --- | --- |
| `compact_block_ibd` | gossip announce / compact wire path |
| `getblock_request_response` | direct `/agora/getblock/1` roundtrip |

## Mempool

`Mempool::admit` verifies secp256k1 signatures, rejects coinbase-shaped txs (`inputs` empty), and reserves input outpoints so two pool txs cannot double-spend. `get_by_short_id` supports compact inflation.

`agora-node` runs `validate_mempool_tx` (live `cf_utxo` + mempool reserved set) under the same lock before admit on both RPC `agora_submitTransaction` and gossip `Transaction` messages. Missing, foreign, overspending, or already-reserved inputs are rejected at the edge. The implicit fee must be ≥ `AGORA_MIN_RELAY_FEE` (default 1); admission stores the fee for template ordering.

Mining templates pull up to `DEFAULT_TEMPLATE_TX_LIMIT` (128) transfers via `select_transfers` (fee descending, then `tx_id`) after the coinbase. Coinbase value is emission plus those transfer fees. On block admit (RPC or gossip), `evict_for_block` drops included txs and any remaining conflicts on the same outpoints.

## Runtime

`NetworkNode::build` constructs a swarm, subscribes to both topics, and emits `NetworkEvent`s (`Listening`, `PeerConnected`, `Message`, …).  
`NetworkNode::run` drives the swarm loop.

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

`scripts/local_testnet.sh` boots a seeder + two nodes with separate datadirs / listen / RPC ports. Peer IDs are ephemeral each boot, so discovery goes through the seeder (not a hardcoded `/p2p/<id>` bootstrap).

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
./scripts/local_testnet.sh smoke-ibd       # mine 1 block on A, wait for B tip converge
```

The script prefers `target/debug/<bin>` when present.

**Smoke proof (`smoke-ibd`):** mines one RandomX block against node-a (`AGORA_MINE_MAX_BLOCKS=1`), then polls until node-b’s tip set matches (CompactBlock / `GetBlock` IBD). Do **not** use `agora_fundAddress` as the gossip check — it only mints locally on the node that handles the RPC.

Optional tx gossip: `agora_submitTransaction` on A → `agora_getTransaction` on B returns `status: "pending"`.

## Follow-ons

- [x] Mempool UTXO pre-checks before gossip admission
- [x] Coinbase outputs in mining templates
- [x] Mempool transfers in mining templates + eviction on admit
- [x] Two-node local seeder + gossip/IBD runbook
- [x] Automated mined-block IBD smoke (`smoke-ibd`)
