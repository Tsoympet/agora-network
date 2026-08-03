# Public testnet operations

Agora **testnet** is RandomX-only (`ChainParams::testnet`). Genesis Block 0 stays
easy (`bits: 0`, hash `afe59232…`) so the frozen root is unchanged; **post-genesis**
templates floor at `daa_min_level = 8` leading-zero bits.

## Quick start (Docker)

```bash
docker compose up --build seeder node-a
# optional second peer
docker compose up --build node-b
# mine a few blocks
docker compose run --rm miner
# faucet (requires AGORA_RPC_ALLOW_FUND on node-a)
docker compose up faucet
```

Host RPC: `http://127.0.0.1:8545/rpc`.

## Multi-host bootstrap

1. Run a seeder on a public IP (`AGORA_SEEDER_BIND=0.0.0.0:18080`). Set `AGORA_SEEDER_TOKEN` so `POST /peers` requires Bearer auth.
2. Start node-a with:
   - `AGORA_LISTEN=/ip4/0.0.0.0/tcp/16111`
   - `AGORA_DNS_SEEDER=http://<seeder-host>:18080`
   - `AGORA_RPC_ALLOW_PUBLIC_BIND=1`
   - Prefer `AGORA_RPC_TOKEN=<secret>` on any non-loopback RPC
3. Peers dial the seeder phonebook (HTTP JSON), then gossip + headers-first IBD.
4. Publish the seeder URL and genesis hash in release notes.

TLS: terminate HTTPS for RPC with a reverse proxy (Caddy/nginx); keep node bind
on localhost or a private interface behind the proxy. P2P remains libp2p TCP.

## PoW policy (A6)

| Network | Algorithm | Notes |
| --- | --- | --- |
| testnet / mainnet (planned) | **RandomX only** | `AGORA_POW_ALGO` ignored |
| dev | RandomX default; kHeavyHash via env | Lab / stratum experiments |

## Faucet policy (A7)

- Lab faucet still calls `agora_fundAddress` (mint). Cap with `AGORA_FAUCET_MAX_TOTAL`.
- For a public testnet, fund a treasury UTXO from premine and replace mint with
  signed spends (follow-up). Keep `agora_fundAddress` off on mainnet (already hard-disabled).

## Orphans

Orphan bodies persist under Warm `orphan/*` and reload on restart so IBD can continue
after a bounce.
