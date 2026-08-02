# P2P (`agora-p2p`)

Network communications for Agora **must** use `libp2p`.

## Topics (v1)

| Topic | Payload |
| --- | --- |
| `agora/blocks/1` | Serialized blocks / compact block announcements |
| `agora/txs/1` | Mempool transactions |

## Phase 4 scope

- Identity / noise transport
- Gossipsub mesh tuning for sub-second DAGs
- Mempool admission aligned with consensus validation
- IBD / compact block relay
