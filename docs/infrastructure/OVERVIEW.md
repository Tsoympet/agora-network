# Infrastructure services

External services that sit beside the Agora node.

| Service | Crate / binary | Role |
| --- | --- | --- |
| DNS seeder | `agora-dns-seeder` | HTTP peer phonebook (`/peers`, `/health`) |
| Stratum pool | `agora-stratum-pool` | kHeavyHash ASIC share aggregation (JSON-lines TCP) |
| Testnet faucet | `agora-testnet-faucet` | Rate-limited address funding (`/drip`, `/balance`) |

## Stratum pool

- Bind: `AGORA_STRATUM_BIND` (default `0.0.0.0:3333`)
- Methods: `mining.subscribe`, `mining.authorize`, `mining.submit`
- PoW hash is a SHA-256 stand-in until audited kHeavyHash FFI is linked

```bash
cargo run -p agora-stratum-pool
```

## Testnet faucet

- Bind: `AGORA_FAUCET_BIND` (default `127.0.0.1:18081`)
- Drip size: `AGORA_FAUCET_DRIP` (base units; default `1000000000` = 10 AGORA)
- Cooldown: `AGORA_FAUCET_COOLDOWN_SECS` (default `60`)
- Credits balances through `agora-rpc`'s `InMemoryBackend` / `fund_address`

```bash
cargo run -p agora-testnet-faucet
curl -X POST http://127.0.0.1:18081/drip \
  -H 'content-type: application/json' \
  -d '{"address":"0102030405060708090a0b0c0d0e0f1011121314"}'
```
