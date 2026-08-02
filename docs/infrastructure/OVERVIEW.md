# Infrastructure services

External services that sit beside the Agora node.

| Service | Crate / binary | Role |
| --- | --- | --- |
| DNS seeder | `agora-dns-seeder` | HTTP peer phonebook (`/peers`, `/health`) |
| Stratum pool | `agora-stratum-pool` | kHeavyHash ASIC share aggregation (JSON-lines TCP) |
| Testnet faucet | `agora-testnet-faucet` | Rate-limited address funding (`/drip`, `/balance`) |

## Stratum pool

- Bind: `AGORA_STRATUM_BIND` (default `0.0.0.0:3333`)
- RPC: `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`) — polls `agora_getBlockTemplate`, submits network shares via `agora_submitBlock`
- Poll: `AGORA_STRATUM_POLL_MS` (default `2000`)
- Methods: `mining.subscribe`, `mining.authorize`, `mining.submit`
- `mining.notify` after subscribe/authorize **and** broadcast to all sessions when a new template is installed
- PoW hash uses audited Kaspa kHeavyHash via `agora_consensus::KHeavyHashPowHasher`
- Run the node with `AGORA_POW_ALGO=kheavyhash` so submitted blocks verify

```bash
AGORA_POW_ALGO=kheavyhash AGORA_TEMPLATE_BITS=1 cargo run -p agora-node
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-stratum-pool
```

## Testnet faucet

- Bind: `AGORA_FAUCET_BIND` (default `127.0.0.1:18081`)
- Drip size: `AGORA_FAUCET_DRIP` (base units; default `1000000000` = 10 AGORA)
- Cooldown: `AGORA_FAUCET_COOLDOWN_SECS` (default `60`)
- RPC: `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`) — calls `agora_fundAddress` / `agora_getBalance`
- Node must enable `AGORA_RPC_ALLOW_FUND=1` so drips mint spendable `cf_utxo` outputs (not overlay balances)

```bash
AGORA_RPC_ALLOW_FUND=1 cargo run -p agora-node
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-testnet-faucet
curl -X POST http://127.0.0.1:18081/drip \
  -H 'content-type: application/json' \
  -d '{"address":"0102030405060708090a0b0c0d0e0f1011121314"}'
```
