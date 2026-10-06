# OVL-EVM-v1

**Maturity:** Experimental. Dev/test genesis gate only.
**Parity:** `OVOLOS ETHEREUM FUNCTIONAL PARITY — INCOMPLETE`.

OVL stays a protocol-native Trident asset. The EVM does not create a sidechain,
does not replace finality, and does not give DRC or TLT a virtual machine.
Finality remains a TLT PoW work threshold and independent two-thirds OVL stake
and two-thirds DRC stake.

## Pin

- Engine: `revm` 42.0.1, `SpecId::SHANGHAI`, constructed with `Context::new`.
- `Context::mainnet()` is not used.
- Solidity fixtures are compiled with solc 0.8.30 `--evm-version shanghai`.
- Numeric chain ids: dev `74000`, testnet `74001`. Ethereum `1` and BSC `56`
  are rejected. A production id is not allocated.
- Machine-readable list of EIPs, precompiles, transaction types, RPC methods,
  decimals, and exclusions: `core/crates/ovl-evm/profile/ovl-evm-v1.json`.

The cryptographic precompiles of this target are the pinned engine's
implementations: Keccak-256, secp256k1 ECDSA recovery, SHA-256, RIPEMD-160,
MODEXP, and the alt_bn128 precompiles. Transaction and consensus signatures
outside the EVM remain secp256k1. BLS12-381, secp256r1, and KZG are excluded.

## Addresses

An Agora address is the first 20 bytes of SHA-256 over a compressed secp256k1
public key, encoded with Bech32m. An Ethereum address is the last 20 bytes of
Keccak-256 over the uncompressed public key without the `0x04` prefix. Those
20-byte values are not aliases. Copying one into the other account family is
incorrect.

EVM accounts live under `meta/ovl/evm/v1/world`, not under
`account/<wire>/<20>`. A colliding 20-byte string is still a different record.

## Amounts and supply

EVM balances are `U256` wei with 18 decimals. The symbol is `OVL`.

Schema 22 keeps the existing `u64` Agora OVL account, stake, treasury, and
supply integers. That integer is the exact quotient `wei / 10^10`. It is not a
second currency. Dust that is not a multiple of `10^10` exists only in the EVM
world. A transaction that claims an Agora-keyed quotient into an Ethereum
sender is not implemented, so those quotients are not silently spendable as gas.

OVL has no live maximum at schema 22. The historical 21 billion whole-unit
figure remains in the draft genesis as a recorded experimental cap and is not
enforced. No Ethereum-style block subsidy or validator issuance curve was
added. The existing staking-reserve drip is the historical predetermined
reserve. The EVM beneficiary receives only the priority tip; it does not mint.

The global genesis artifact still says 8 decimals because TLT and DRC stay on
that scale. OVL wei is applied at the schema-22 boundary.

## Fees

The market is EIP-1559 in OVL wei:

- target gas = `gas_limit / elasticity_multiplier`
- base fee rises or falls by `parent * delta / target / denominator`, with a
  minimum increase of 1 wei when usage is above target
- 100% of `base_fee * gas_used` is burned inside the execution world
- the priority tip is credited to the genesis-configured Ethereum beneficiary
- dev defaults are 1 gwei, 30,000,000 gas, elasticity 2, denominator 8

Validators cannot override the formula. Version-1 Agora-signed execution still
credits its intrinsic fee to the OVL reward pool. Version 2 does not also
credit that pool.

## Transactions and activation

Accepted raw envelopes are legacy EIP-155, EIP-2930 type 1, and EIP-1559 type
2. Unprotected legacy transactions and high-`s` signatures are rejected. Type 3
and type 4 are rejected.

Borsh remains the outer consensus encoding. Version 2 of `OvlExecutionTx`
carries the raw RLP bytes in `data` and leaves the Agora public key and
signature empty. The Agora-signed mempool rejects that version. Inclusion is
the block lane, and only after a dev or test world has been activated. The
historical rollup's fixed funded caller, unsigned compact transactions, and
unknown-root reseeding are not accepted.

The selected Trident block supplies the EVM parent hash, timestamp in seconds,
and block hash. `PREVRANDAO` is that block hash. The EVM block number counts
applied blocks that carried a raw transaction. The beneficiary is the address
configured when the gate was activated. The EVM does not choose the canonical
chain or finality.

## State root

Schema 22 folds `agora-ovl-evm-commitment-v1` into the Trident state root. The
execution subroot is SHA-256 over a domain-separated Borsh image of accounts,
receipts, supply, and the EVM block. It is not an Ethereum Merkle-Patricia
state root, and `eth_getProof` is not implemented.

Contract-verification notes may sit on the world record and are excluded from
the subroot. There is no verification submit API.

## RPC and index

`eth_*`, `net_*`, and `web3_clientVersion` are answered from the stored world
by one dispatcher. `eth_sendRawTransaction` validates and stores a process-local
pending inbox. Receipts appear only after the block lane applies the raw
transaction. `eth_syncing` false means this executor is not reporting a sync
gap. `net_listening` is false and `net_peerCount` is zero in this adapter.
`eth_maxPriorityFeePerGas` returns `0x1` as a read suggestion, not a consensus
override.

The receipt indexer rebuilds ERC-20, ERC-721, and ERC-1155 transfer logs from
canonical receipts. That is not a wallet certification and not an explorer UI.

## Oracles, applications, and bridges

Oracles, DAOs, AMMs, NFT markets, proxies, and multisigs are application
contracts. Their outputs are application-level signed data or contract storage.
They are not consensus-injected facts. The checked-in fixtures execute; they
are not protocol balances and they are not audited production deployments.

A wrapped-OVL contract may later exist. It would be an application token. It
is not native OVL and it is not the gas asset.

Native DRC↔OVL exchange is not implemented. It needs an explicit atomic
settlement rule in both lanes. An ERC-20 wrapper is not that rule.

A bridge, if one is ever specified, has to be a verifiable light-client or
threshold-attested proof over finalized Trident state, with an explicit asset
mint/burn or lock on each side and no custodian key in consensus. This program
does not implement that bridge and does not add a centralized signer.

## What DRC and TLT are not

DRC remains the contract-free payment and settlement asset: accounts, payments,
tags, DepositAuth, expiry, regular keys, multisign, master disable, Tickets,
escrow, Checks, payment channels, trust lines, issued assets,
authorization/freeze/clawback, accepted-only fee burn, and the schema-21
ledger-object index. It has no bytecode, deploy, call, or hook.

TLT remains the only RandomX asset. Neither asset is accepted as EVM gas, as
an RPC execution selector, or as a version-2 payload.
