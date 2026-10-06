# DRC accepted-transaction fee burning

**Maturity:** Experimental.

Trident protocol v22 destroys the exact signed DRC fee of every accepted,
fee-bearing DRC state transition. This is an XRPL-inspired economic rule adapted
to Agora's canonical BlockDAG state machine; it is not XRPL result-code, wire,
address, or consensus compatibility.

## Consensus rule

Fee attribution happens only after the typed operation has passed authorization,
sequence-or-Ticket, balance, object, and lane-order checks:

```text
if acceptance == Accepted:
    lifetime_burned_DRC' = lifetime_burned_DRC + signed_fee
    net_supply_DRC       = issued_supply_DRC - lifetime_burned_DRC'
else:
    lifetime_burned_DRC' = lifetime_burned_DRC
```

`ExactDuplicate`, `ConflictLost`, malformed, expired, unauthorized, and rejected
operations burn nothing. Zero-fee accepted fixtures leave the counter unchanged.
Overflow or `burned_supply > issued_supply` fails closed before the operation
batch is committed.

The operation mutation, account debit, burned counter, acceptance record, and
reorg journal are committed atomically. The journal snapshots the counter before
each accepted fee, so reverse-order rollback restores it exactly. DRC fees no
longer credit `stake/reward_pool/DRC`; reserve drips and slash proceeds continue
to use that pool. TLT UTXO/miner fee behavior and OVL execution fee attribution
are unchanged.

## Supply accounting

Each native asset has persisted `issued_supply` and `burned_supply` counters.
`net_supply` is checked and derived as `issued - burned`; it is not an
independently mutable value. The state root commits maximum, issued, burned, and
net values for TLT, OVL, and DRC under
`agora-native-supply-root-v1`. Only DRC transaction fees use the burn transition
in protocol v22.

`agora_getNativeAssetSupply` is a public, read-only query accepting `TLT`, `OVL`,
or `DRC`. It returns decimal strings for `maximum_supply`, `issued_supply`,
`burned_supply`, and `net_supply` so JavaScript clients do not lose `u64`
precision.

## Activation and migration

- Fee-burn activation protocol: `22`
- Fee-burn activation state transition: `agora-trident-state-v20`
- Transaction signing profile: unchanged `agora-trident-tx-v9`
- Fee-burn activation state-root domain: `agora-trident-state-root-v14`
- Fee-burn activation datadir schema: `20`
- Current aggregate profile: protocol `24`, state transition
  `agora-trident-state-v22`, state-root domain
  `agora-trident-state-root-v16`, datadir schema `21`

Fresh genesis initializes every burned counter to zero. The explicit schema
19-to-20 library migration also initializes zero atomically and refuses other
source versions, missing schema-20 counters, or unexpected preexisting
schema-19 burn state. It never derives historical burns from the DRC reward
pool because that pool mixes fees, reserve drips, and slash proceeds.

No public activation height is claimed: Trident genesis v3 remains unfrozen and
its live loader remains disabled. A future public-network ceremony must freeze
the activation identity or start from a fresh schema-20 genesis.

This rule does not define DRC issuance rates, validator reward curves, reserve
drips, or community distributions, and it adds no programmable execution
surface.
