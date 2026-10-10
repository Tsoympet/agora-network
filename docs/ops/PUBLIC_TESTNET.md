# Historical v2 public testnet operations

These instructions describe the frozen v2 TLT testnet, not the UNFROZEN
Trident v3 draft. Agora **testnet** is RandomX-only (`ChainParams::testnet`). Genesis Block 0 stays
easy (`bits: 0`, hash `afe59232…`) so the frozen root is unchanged; **post-genesis**
templates floor at `daa_min_level = 8` leading-zero bits.

Offline v3 draft/freeze-readiness commands are documented in
[`../genesis/README.md`](../genesis/README.md). They cannot boot Trident, and
their existence does not make Trident Public testnet ready.

Do not point `AGORA_GENESIS_FILE` at the checked-in Trident draft. That file is
UNFROZEN and fails freeze-ready. Node v2 loading remains `AGORA_GENESIS_FILE`
plus `prepare_legacy_datadir`. A freeze-ready v3 artifact (not the public
draft) can be materialized with `AGORA_TRIDENT_GENESIS_FILE` /
`agora-node genesis trident materialize`; that path writes live balances and
refuses to load a libp2p key until freeze-ready checks pass. Do not combine the
two genesis env vars. Do not reuse one datadir for both protocols.

Public Trident testnet is still **not** declared: the checked-in v3 draft is
UNFROZEN, default docker-compose still boots frozen v2, and a ceremony freeze
of `agora-trident-testnet-1` has not happened. Frozen v2 peers boot, IBD, and
send TLT, but they never finalize.

An **Experimental public-testnet** freeze-ready artifact exists at
[`../genesis/trident.experimental.public-testnet.json`](../genesis/trident.experimental.public-testnet.json).
It uses generated secp256k1 validators, a distinct
`chain_id` (`agora-trident-experimental-testnet-1`), RandomX-only PoW, and
dual-PoS genesis sets so finality can fire after the PoW work threshold. It
is not ceremony-final and not mainnet. Default compose must stay on frozen
v2. Opt in explicitly:

```bash
docker compose --profile experimental-trident up --build seeder experimental-trident-a
```

Host RPC for that profile: `http://127.0.0.1:8555/rpc`. Clear
`AGORA_GENESIS_FILE` when setting `AGORA_TRIDENT_GENESIS_FILE`.

### Local host demonstration (this tree)

`scripts/experimental_trident_mesh.sh` boots a seeder plus two
`agora-node` processes against the Experimental artifact without Docker.

Recorded on this host (2026-10-10), `agora-node --no-default-features`
(in-memory store; RandomX hasher falls back to SHA-256 because
`libstdc++` headers are missing and `rust-randomx` / default
`rocksdb+randomx` do not compile):

| Step | Result |
| --- | --- |
| Boot | Both nodes materialized freeze-ready Block 0. `chain_id` = `agora-trident-experimental-testnet-1`. Gossip `genesis_hash` = `d81e0f9c…` (live `BlockHeader` id). `artifact_identity` = `68b3c7af…`. Fingerprint prefix `52b5fc85…`. |
| Peers | First-boot simultaneous seeder dials failed Noise handshake. Stagger / `AGORA_BOOTSTRAP` from B→A connected (`connected_peers=1`). Headers-first IBD at genesis reported the peer was not ahead. |
| Tx | Device-local TLT spend from generated key `0xa3…` admitted on A (`pending`). Two-peer gossipsub did **not** put the tx in B’s mempool. |
| Mine + IBD | `agora-miner --no-default-features` found a SHA-256 fallback solution at `daa_min_level=8` (`00997c73…`). B’s tip set matched A. The spend was `confirmed` on B via the block (not via mempool gossip). |
| Dual-PoS | `agora_getFinality` returned a signable checkpoint body. OVL (`0xa1…`) then DRC (`0xa2…`) attestations finalized the tip on A; B converged to `Finalized` over gossip. Admit still uses `FinalityPowPolicy::default()` (`min_pow_depth=1`), not the artifact’s threshold `8`. |

This is a local Experimental lab mesh. It is **not** Public testnet, not
ceremony-final, and not a RandomX public-network proof. Default compose
stays on frozen v2.

```bash
cargo build -p agora-node --no-default-features -p agora-dns-seeder
cargo build -p agora-miner-sidecar --no-default-features
./scripts/experimental_trident_mesh.sh wipe
./scripts/experimental_trident_mesh.sh seeder          # terminal 0
./scripts/experimental_trident_mesh.sh node-a          # terminal 1
# wait until A is listening, then:
AGORA_BOOTSTRAP=/ip4/127.0.0.1/tcp/16121/p2p/<node-a-peerid> \
  ./scripts/experimental_trident_mesh.sh node-b        # terminal 2
./scripts/experimental_trident_mesh.sh wait-peers
./scripts/experimental_trident_mesh.sh smoke-tx        # A admit; B mempool may miss
./scripts/experimental_trident_mesh.sh smoke-ibd       # mine 1 + tip converge
./scripts/experimental_trident_mesh.sh smoke-finality  # OVL+DRC attest
```

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
