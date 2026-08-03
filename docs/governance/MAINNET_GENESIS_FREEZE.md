# Mainnet genesis freeze

Mainnet refuses boot until a frozen Block 0 is published. This checklist turns
`docs/genesis/mainnet.genesis.draft.json` into a real `mainnet.genesis.json`.

## Preconditions

- [ ] Phase 32 genesis v2 fields agreed (tokens, consensus, wallet)
- [ ] SLIP-0044 coin type registered (or explicitly accepted provisional risk) — see [`SLIP0044.md`](SLIP0044.md)
- [ ] Premine / treasury addresses chosen and published
- [ ] Freeze timestamp chosen (UTC ms)
- [ ] Initial `bits` / DAA floor agreed (`ChainParams::mainnet` defaults: bits=16, `daa_min_level=8`)

## Economic freeze (do not reopen lightly)

| Ticker | Max supply (whole) | Layer |
| --- | --- | --- |
| TLT | 100,000,000 | L1 native |
| DRC | 6,000,000,000 | L2+ registry |
| OVL | 21,000,000,000 | L2 registry |

L1 consensus only mints **TLT**. DRC/OVL caps stay registry-only until their layers issue.

## Freeze procedure

```bash
# 1. Set final ChainParams::mainnet() values in
#    core/crates/state-machine/src/network.rs
#    (premine address, timestamp_ms, bits, expected_genesis after compute)

# 2. Allow mainnet boot in for_network() once expected_genesis is set

# 3. Dump artifact
cargo run -p agora-node -- genesis dump --network mainnet \
  --out docs/genesis/mainnet.genesis.json

# 4. Verify
cargo run -p agora-node -- genesis verify --network mainnet \
  --file docs/genesis/mainnet.genesis.json

# 5. Embed MAINNET_GENESIS_HASH_HEX (mirror TESTNET_GENESIS_HASH_HEX)
# 6. Tag release, publish artifact hash, wipe incompatible datadirs
```

Helper script (fills timestamps / reminds checklist):

```bash
./scripts/prepare_mainnet_genesis.sh
```

## After freeze

- `AGORA_NETWORK=mainnet` boots only when datadir genesis matches the constant
- `agora_fundAddress` remains permanently disabled
- Env overrides for bits / PoW / premine stay **off** on mainnet
