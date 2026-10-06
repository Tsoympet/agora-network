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

No smart contract, RPC administrator, or ordinary transaction may mint TLT,
OVL, or DRC outside protocol-defined issuance.

## Ledger placement

| Asset | Primary state | Notes |
| --- | --- | --- |
| TLT | UTXO set | Only mineable asset; coinbase + base fees |
| OVL | Account module plus the schema-22 EVM world | Sole smart-contract/VM execution and gas domain; validator collateral; native OVL is not an ERC-20. Agora keys and Ethereum keys are different derivations |
| DRC | Account module | Contract-free payments and typed settlement state; validator collateral; not a stablecoin by default |

Cross-asset input/output mismatch → `Invalid`.

## Programmability boundary

| Asset | User-programmable execution | Protocol-native typed operations |
| --- | --- | --- |
| TLT | No | UTXO transfer, coinbase, base-fee settlement |
| OVL | **Yes, exclusively** (subject to versioned activation) | Account transfer, staking, execution envelopes |
| DRC | **No** | Payments, escrow, Checks, payment channels, trust lines, issued assets and controls, freeze/clawback, multisign, Tickets |

DRC has no VM, bytecode, deploy/call, Hook, script, callback, or
contract-facing API. A generic endpoint must reject any request that attempts
to select DRC for execution. DRC operation envelopes are closed schemas;
unrecognized executable fields fail deserialization rather than being ignored.

## Wallet / RPC amounts

Use `bigint` / decimal strings in TypeScript. Never use JavaScript `number` for `u64` monetary values.
