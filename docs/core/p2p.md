# P2P (`agora-p2p`)

Network communications for Agora **must** use `libp2p`.

## Stack

- Transport: TCP + Noise + Yamux
- Pub/sub: Gossipsub (strict validation, signed messages)
- Payloads: `borsh`-encoded `NetworkMessage`
- Topics and mempool admission are bound to `NetworkFingerprint`

## Topics

| Topic | Payload |
| --- | --- |
| `agora/<fingerprint-hex>/blocks/1` | `Block` / `BlockAnnounce` |
| `agora/<fingerprint-hex>/txs/1` | `Transaction` |

Legacy constants `TOPIC_BLOCKS` / `TOPIC_TRANSACTIONS` remain for reference only — runtime uses fingerprint-bound helpers.

## Mempool

- `Mempool::admit(tx, fingerprint)` verifies secp256k1 signatures in the fingerprint domain.
- `Mempool::evict_by_acceptance(result)` removes accepted tx ids and any mempool txs that spend outpoints spent by the acceptance journal. Eviction is driven by **acceptance**, not block color.

## Runtime

`NetworkConfig` carries the fingerprint. `NetworkNode::build` subscribes/publishes on fingerprint-scoped topics.

Integration test `two_node_gossip` dials two local nodes on the same fingerprint and exchanges a signed transaction.

## DNS seeder

`infrastructure/dns-seeder` (`agora-dns-seeder`) is an HTTP phonebook:

| Endpoint | Purpose |
| --- | --- |
| `GET /peers` | JSON array of multiaddrs |
| `POST /peers` | Register a multiaddr |
| `GET /health` | Liveness |

Bind with `AGORA_SEEDER_BIND` (default `127.0.0.1:18080`). Preload via `AGORA_SEEDER_PEERS`.
