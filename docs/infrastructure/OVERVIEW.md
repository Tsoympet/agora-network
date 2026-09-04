# Infrastructure services

External services that sit beside the Agora node.

| Service | Crate / binary | Role |
| --- | --- | --- |
| DNS seeder | `agora-dns-seeder` | HTTP peer phonebook (`/peers`, `/health`); two-node local via `scripts/local_testnet.sh seeder` |
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
- Drip size: `AGORA_FAUCET_DRIP` (base units; default `1000000000` = 10 TLT)
- Cooldown: `AGORA_FAUCET_COOLDOWN_SECS` (default `60`)
- Default mode: `AGORA_FAUCET_MODE=treasury` — signs ordinary spends from
  the BIP-44 external account selected by `AGORA_FAUCET_MNEMONIC`
- RPC: `AGORA_RPC_URL` (default `http://127.0.0.1:8545/rpc`) — treasury mode
  calls `agora_getUtxos`, `agora_submitTransaction`, and `agora_getBalance`
- Lab-only mode: `AGORA_FAUCET_MODE=mint` calls `agora_fundAddress` and requires
  node `AGORA_RPC_ALLOW_FUND=1`; never enable it on a shared Trident testnet

```bash
AGORA_NETWORK=testnet cargo run -p agora-node
AGORA_FAUCET_MODE=treasury \
  AGORA_RPC_URL=http://127.0.0.1:8545/rpc \
  cargo run -p agora-testnet-faucet
curl -X POST http://127.0.0.1:18081/drip \
  -H 'content-type: application/json' \
  -d '{"address":"0102030405060708090a0b0c0d0e0f1011121314"}'
```
