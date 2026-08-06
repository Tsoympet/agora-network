# Monetary Policy (Trident L1)

**Maturity:** Scaffold. Caps below are **working defaults** carried from current genesis marks; a Trident genesis ceremony may revise them only via explicit consensus upgrade.

## Working supply caps (base units, 8 decimals)

| Asset | Whole units | Base units (`u64`) |
| --- | --- | --- |
| TLT | 100,000,000 | `10_000_000_000_000_000` |
| OVL | 21,000,000,000 | `2_100_000_000_000_000_000` |
| DRC | 6,000,000,000 | `600_000_000_000_000_000` |

## TLT

- Fixed maximum supply.
- Only TLT RandomX block production may issue new TLT after genesis.
- Deterministic halving / emission (current L1 schedule: initial 50 TLT, halving every 210_000 blue-score units — preserve unless ceremony changes).
- Miner subsidy + accepted TLT-denominated base fees.
- No PoS issuance of TLT.
- No governance minting.

## OVL

- Fixed maximum supply.
- No mining.
- Genesis allocation + deterministic staking-reward reserve.
- Staking rewards may come from: predetermined staking emissions, OVL execution fees, slashing proceeds.
- No unrestricted administrator minting.
- Supply-policy changes require explicit consensus upgrade / fork.

## DRC

- Fixed maximum supply.
- No mining.
- Genesis allocation + deterministic validator/community reward reserve.
- Rewards may come from: predetermined staking emissions, DRC payment fees, slashing proceeds.
- **Not** described as a stablecoin unless a separately audited stabilization system exists.
- No unrestricted administrator minting.

## Invariants (must be tested)

```text
issued_supply(asset) <= maximum_supply(asset)

sum(
  account_balances + utxos + staking_locks + unbonding
  + treasuries + escrow + burned_accounting
) == expected_supply_state(asset)
```

## Fee entitlement (policy hooks)

Documented splits (exact BPS in genesis / consensus policy):

| Fee class | Default primary recipient | Optional sinks |
| --- | --- | --- |
| TLT base | TLT miner (coinbase) | Security treasury, burn |
| OVL execution | OVL validators (pro-rata) | Builder treasury, burn |
| DRC payment | DRC validators (pro-rata) | Community treasury, burn |

Credits apply only when the paying transaction’s acceptance status is `Accepted`.
