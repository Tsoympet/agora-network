# P2P (`agora-p2p`)

Network communications for Agora **must** use `libp2p`.

## Stack

- Transport: TCP + Noise + Yamux
- Pub/sub: Gossipsub (strict validation, signed messages)
- Payloads: `borsh`-encoded `NetworkMessage`

## Topics (v1)

| Topic | Payload |
| --- | --- |
| `agora/blocks/1` | `Block` / `BlockAnnounce` / `CompactBlock` / `GetBlock` |
| `agora/txs/1` | `Transaction` |

## Compact blocks / IBD

After a block is admitted locally, `agora-node` gossips:

1. `CompactBlock { header, short_ids }` — BIP152-style short ids (first 8 bytes of each `tx_id`)
2. `BlockAnnounce { hash }` — hash-only tip signal

Receivers try `reconstruct_compact_block` against the local mempool. On miss (or hash-only announce without a body), they publish `GetBlock { hash }` (deduped via `PendingFetches`). Peers that hold the block reply with a full `Block`.

Empty-tx templates reconstruct immediately (no mempool lookup). Integration test: `compact_block_ibd`.

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
2. On first `Listening` event, `register_with_seeder` POSTs the dialable multiaddr (`0.0.0.0` → `127.0.0.1`)

```bash
cargo run -p agora-dns-seeder
AGORA_DNS_SEEDER=http://127.0.0.1:18080 cargo run -p agora-node
```

## Follow-ons

- Request-response transport for `GetBlock` (avoid mesh-wide full-block replies)
- Peer scoring & mesh tuning for sub-second DAGs
- Periodic seeder refresh / re-register
