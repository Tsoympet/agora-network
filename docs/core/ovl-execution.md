# Native OVL execution boundary

**Maturity:** Experimental.

Trident Phase 4a introduces `OvlExecutionTx`, a secp256k1-signed,
chain/genesis-bound L1 envelope carried in `Block.ovl_executions`.

## Active transition

The initial boundary deliberately supports only gas-metered EOA value calls:

- canonical balance source: the existing OVL account ledger
- intrinsic gas: `21_000`
- fee: `intrinsic_gas × max_fee_per_gas`, paid in OVL
- debit: `value + fee`; recipient receives `value`
- accepted fee: credited to the OVL validator reward pool
- nonce: shared with OVL account transfers and stake operations

All arithmetic and recipient overflow checks occur before mutation. The
transition returns a deterministic receipt and journals account/reward-pool
state for BlockDAG reorgs.

## Intentionally inactive

- `to == Address::ZERO` contract creation
- non-empty call data / contract execution
- persistent contract code or storage
- `eth_call` compatibility

These requests fail consensus validation instead of being treated as successful
no-ops. The legacy `agora-ovolos-rollup` `fund_caller`, compact unsigned
encoding, OVL PoW, and second OVL ledger are not part of this L1 path.

## Wire and acceptance

`agora_submitOvlExecution` validates and reserves the sender's shared OVL nonce,
then gossips the envelope for mining-template inclusion. Non-empty execution
lanes use the `agora-block-body-v3` commitment and full-block gossip. Acceptance
is recorded in `BlockAcceptanceRecord.execution_statuses`.

This is not Ethereum-equivalent. A deterministic VM and contract state root
require a later explicitly versioned transition.
