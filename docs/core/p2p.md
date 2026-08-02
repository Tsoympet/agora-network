# P2P (`agora-p2p`)

Network communications for Agora **must** use `libp2p`.

## Stack

- Transport: TCP + Noise + Yamux
- Pub/sub: Gossipsub (strict validation, signed messages)
- Payloads: `borsh`-encoded `NetworkMessage`

## Topics (v1)

| Topic | Payload |
| --- | --- |
| `agora/blocks/1` | `Block` / `BlockAnnounce` |
| `agora/txs/1` | `Transaction` |

## Mempool

`Mempool::admit` verifies secp256k1 signatures via `agora-crypto` before accepting a tx. Unsigned or invalid txs are rejected at the gossip edge.

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

## Follow-ons

- Pull peers from seeder URL inside `NetworkConfig::dns_seeder_url`
- IBD / compact block fetch protocol
- Peer scoring & mesh tuning for sub-second DAGs
