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

`Mempool::admit` verifies secp256k1 signatures via `agora-crypto` before accepting a tx. Unsigned or invalid txs are rejected at the gossip edge. `get_by_short_id` supports compact inflation.

## Runtime

`NetworkNode::build` constructs a swarm, subscribes to both topics, and emits `NetworkEvent`s (`Listening`, `PeerConnected`, `Message`, …).  
`NetworkNode::run` drives the swarm loop.

Integration test `two_node_gossip` dials two local nodes and exchanges a signed transaction.

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

## Follow-ons

- Connection-limits behaviour tied to `max_peers`
