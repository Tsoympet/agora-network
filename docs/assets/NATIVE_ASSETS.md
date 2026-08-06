# Native Assets (Trident L1)

**Maturity:** Scaffold (Phase 1 types target).

## Identifier

```rust
pub enum NativeAssetId {
    TLT = 0x00,
    OVL = 0x01,
    DRC = 0x02,
}
```

Stable serialized representation is the single byte above. Every native value entry must identify its asset.

```rust
pub struct NativeAmount {
    pub asset: NativeAssetId,
    pub value: Amount, // u64 base units, 8 decimals by default
}
```

## Per-asset enforcement

The state transition independently enforces for each asset:

- Maximum supply and current issued supply
- Emission / distribution schedule
- Transfer rules and fee rules
- Staking, delegated, unbonding, slashed balances
- Treasury, burned, vesting, governance locks
- Account or UTXO nonces where applicable

No smart contract, RPC administrator, or ordinary transaction may mint TLT, OVL, or DRC outside protocol-defined issuance.

## Ledger placement

| Asset | Primary state | Notes |
| --- | --- | --- |
| TLT | UTXO set | Only mineable asset; coinbase + base fees |
| OVL | Account module | Execution gas; validator collateral; **one** balance definition |
| DRC | Account module | Payments; validator collateral; not a stablecoin by default |

Cross-asset input/output mismatch → `Invalid`.

## Wallet / RPC amounts

Use `bigint` / decimal strings in TypeScript. Never use JavaScript `number` for `u64` monetary values.
