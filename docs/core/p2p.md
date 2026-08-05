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

- `Mempool::admit(tx, fingerprint, utxo, tip_blue_score)` runs `precheck_regular_tx` (fingerprint signature, ownership, maturity, min fee, size/input caps) and tracks claimed outpoints (first admit wins).
- `Mempool::evict_by_acceptance(result)` removes accepted tx ids and any mempool txs that spend outpoints spent by the acceptance journal. Eviction is driven by **acceptance**, not block color.

## Runtime

`NetworkConfig` carries fingerprint, `max_peers`, bounded event/command channel capacities, and optional `identity_path` (persistent ed25519 key via `load_or_create_identity`).

`NetworkNode::build` composes gossipsub with `libp2p::connection_limits` (`max_peers`), uses SHA-256 message IDs, and drops oversized / topic-mismatched gossip.

Integration test `two_node_gossip` dials two local nodes on the same fingerprint and exchanges a signed transaction.

## DNS seeder

`infrastructure/dns-seeder` (`agora-dns-seeder`) is an HTTP phonebook:

| Endpoint | Purpose |
| --- | --- |
| `GET /peers` | JSON array of multiaddrs |
| `POST /peers` | Register a multiaddr |
| `GET /health` | Liveness |

Bind with `AGORA_SEEDER_BIND` (default `127.0.0.1:18080`). Preload via `AGORA_SEEDER_PEERS`.
