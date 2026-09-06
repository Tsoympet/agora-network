# Historical v2 public testnet operations

These instructions describe the frozen v2 TLT testnet, not the UNFROZEN
Trident v3 draft. Agora **testnet** is RandomX-only (`ChainParams::testnet`). Genesis Block 0 stays
easy (`bits: 0`, hash `afe59232…`) so the frozen root is unchanged; **post-genesis**
templates floor at `daa_min_level = 8` leading-zero bits.

Offline v3 draft/freeze-readiness commands are documented in
[`../genesis/README.md`](../genesis/README.md). They cannot boot Trident, and
their existence does not make Trident Public testnet ready.

Do not point `AGORA_GENESIS_FILE` at the checked-in Trident draft. Node loading
remains v2-only while Trident runtime policy and Block 0 live-state wiring are
incomplete. A candidate Block 0 Meta envelope can now be staged and verified
without writing live balances. Its versioned Borsh datadir identity is committed
atomically with the envelope and checked byte-for-byte on reopen. The historical
v2 node intentionally refuses any complete or partial Trident identity before
loading/creating `$AGORA_DATA/p2p/identity.key`, accessing a seeder, building a
swarm, or binding RPC; do not try to reuse one datadir for both protocols.

Boot still does not consume the Trident candidate as live state. The remaining
blocker is lossless Block 0 body, UTXO, account, treasury, vesting, validator,
and finality materialization whose recomputed state root equals the committed
header, followed by explicit runtime activation gates. This prerequisite does
not make Trident Public testnet ready.

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

- **Default:** `AGORA_FAUCET_MODE=treasury` — signed spends from BIP-44 external(0)
  (`AGORA_FAUCET_MNEMONIC`, default = testnet premine `abandon … about`).
- Lab opt-in mint: `AGORA_FAUCET_MODE=mint` + node `AGORA_RPC_ALLOW_FUND=1`.
- Cap with `AGORA_FAUCET_MAX_TOTAL`. Keep `agora_fundAddress` off on mainnet
  (already hard-disabled).

## Orphans

Orphan bodies persist under Warm `orphan/*` and reload on restart so IBD can continue
after a bounce.
