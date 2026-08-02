# Governance (`agora-governance`)

Launch-security primitives: quadratic voting and anti-whale caps.

## Quadratic voting

```
EffectiveVotes = floor(sqrt(RawBalance))
```

Uses integer square root only — no floating point in consensus paths.

## Whale protection

Before the square-root transform, a voter’s countable balance is clamped to **5% of total supply** (`WhaleCapConfig::FIVE_PERCENT`, 500 bps):

```
capped = min(raw_balance, total_supply * 5 / 100)
effective = isqrt(capped)
```

This prevents a single holder from converting an outsized balance into unchecked voting power.

## API

| Item | Role |
| --- | --- |
| `quadratic_votes` | Pure √ mapping |
| `apply_whale_cap` | 5% supply clamp |
| `tally_quadratic_votes` | Electorate → `EffectiveVote` rows |
| `max_power_share_bps` | Largest post-tally share |

## Related tests

- Unit tests in `agora-governance`
- Partition stress tool: `cargo run -p agora-consensus --bin ghostdag_fuzzer`
