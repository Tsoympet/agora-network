# OVL Ethereum functional parity

**Status line:** `OVOLOS ETHEREUM FUNCTIONAL PARITY — INCOMPLETE`

**Maturity:** Experimental. Newly added consensus behavior is at most `TESTED`.
Nothing in this profile is `PRODUCTION READY`.

Compatibility target: `OVL-EVM-v1`, pinned `revm` 42.0.1 `SpecId::SHANGHAI`.
Machine-readable profile: `core/crates/ovl-evm/profile/ovl-evm-v1.json`.
Protocol notes: `docs/core/ovl-evm.md`.

Allowed statuses: `NOT IMPLEMENTED`, `IN DEVELOPMENT`, `IMPLEMENTED`, `TESTED`,
`PRODUCTION READY`.

| FEATURE | ETHEREUM REFERENCE | AGORA IMPLEMENTATION | TEST | RPC | DOCUMENTATION | STATUS |
| --- | --- | --- | --- | --- | --- | --- |
| Native OVL, 18-decimal wei | ETH balance, 18 decimals | Stored Agora `u64` is the exact quotient `wei / 10^10`. EVM dust lives in the dev-gated world. No second live unit and no ERC-20 native balance | `legacy_whole_token_scales_by_ten_to_the_tenth`, `one_legacy_unit_is_ten_billion_wei_and_not_a_second_balance` | `eth_getBalance` reads the EVM world | `docs/core/ovl-evm.md` | TESTED |
| Agora-key claim into an EVM account | n/a | Not built. Quotient balances are not Ethereum senders | none | none | `docs/core/ovl-evm.md` | NOT IMPLEMENTED |
| Uncapped OVL supply | no protocol max on ETH | Schema 22 does not enforce the historical 21B cap. No new subsidy curve | `schema_22_does_not_enforce_the_historical_ovl_cap` | none | `docs/assets/MONETARY_POLICY.md` | TESTED |
| Block subsidy / validator issuance | ETH issuance varies by fork | Not configured. Historical staking-reserve drip remains. Coinbase does not mint OVL | supply tests still bound DRC and TLT | none | `docs/core/ovl-evm.md` | NOT IMPLEMENTED |
| EIP-1559 base fee, tip, 100% burn | EIP-1559 | Genesis parameters; validators cannot change the formula. Base fee burned in the world. Tip to the configured beneficiary | `base_fee_matches_eip1559_integer_rules`, `eip1559_burns_base_fee_and_pays_tip_to_beneficiary` | `eth_gasPrice`, `eth_feeHistory`, `eth_maxPriorityFeePerGas` | profile JSON | TESTED |
| Legacy / EIP-2930 / EIP-1559 | EIP-155, EIP-2930, EIP-1559 | Parser, low-s, chain id 74000/74001 | `recovers_legacy_type1_and_type2_and_rejects_exclusions` | `eth_sendRawTransaction` pending only | `docs/core/ovl-evm.md` | TESTED |
| Blob and 7702 transactions | EIP-4844, EIP-7702 | Rejected | same parser test | rejected before admission | profile exclusions | NOT IMPLEMENTED |
| Shanghai CREATE/CALL/STATICCALL/DELEGATECALL/CREATE2 | Shanghai opcodes | Pinned revm, dev gate off by default | `fixtures_execute_create_calls_tokens_and_precompiles` | `eth_call`, `eth_getCode`, `eth_getStorageAt` | `docs/core/ovl-evm.md` | TESTED |
| Keccak-256 | opcode 0x20 | revm | fixture `keccakOf` | `eth_call` | profile | TESTED |
| ecrecover, SHA-256, RIPEMD-160, identity, MODEXP | precompiles 0x01–0x05 | revm | `shanghai_precompiles_are_the_pinned_engine` | `eth_call` | profile | TESTED |
| alt_bn128 pairing | precompile 0x08 | revm; empty input returns success | same precompile test | `eth_call` | profile | TESTED |
| alt_bn128 add/mul, BLAKE2f | 0x06, 0x07, 0x09 | Present in the pinned Shanghai engine | not separately vectored | callable only if the gate is active | profile | IMPLEMENTED |
| BLS12-381, secp256r1, KZG | later forks | Excluded | parser rejects type 3 | none | profile exclusions | NOT IMPLEMENTED |
| Application fixtures | ERC-20/721/1155/4626, multisig, proxy, timelock, AMM | Executable solc 0.8.30 Shanghai bytecode. Not protocol balances and not audited deployments | fixture test and receipt indexer | `eth_getLogs` | `fixtures/Fixtures.sol` | TESTED |
| Canonical `eth_*` / `net_*` / `web3_clientVersion` | execution-layer JSON-RPC | Dispatcher reads the world. `eth_sendRawTransaction` does not commit. Node adapter uses the same dispatcher | `rpc_reads_canonical_state_and_pending_does_not_commit` | listed in the profile | `docs/core/ovl-evm.md` | TESTED |
| `eth_getProof` / MPT | state proof | Not built. Subroot is domain-separated SHA-256 | none | method absent | profile exclusions | NOT IMPLEMENTED |
| `eth_syncing` | sync status | Returns false for this executor. It does not report a public network | RPC test | `eth_syncing` | `docs/core/ovl-evm.md` | TESTED |
| Log/token/NFT index | client index | Rebuilt from canonical receipts | fixture test | `eth_getLogs` | `docs/core/ovl-evm.md` | TESTED |
| Wallet certification | wallet vendor suites | Not run | none | chain metadata is in the profile | this file | NOT IMPLEMENTED |
| Contract-verification workflow | source registry | Storage field exists and is outside the execution subroot. No submit API | none | none | `docs/core/ovl-evm.md` | NOT IMPLEMENTED |
| Reorg, restart, identical replay | consensus determinism | World snapshot plus two stores through the block-lane applier | `reorg_restart_and_two_worlds_share_a_subroot`, `raw_ethereum_lane_is_dev_gated_and_isolated_from_drc_and_tlt` | n/a | `docs/core/ovl-evm.md` | TESTED |
| DRC and TLT cannot enter the VM | n/a | Foreign asset names rejected. Version 2 parses Ethereum bytes only. DRC balances are untouched | engine and state-machine tests | RPC rejects a DRC asset parameter | capability profile | TESTED |
| Public mempool for raw EVM transactions | txpool | Version 2 is rejected by the Agora-signed mempool | `raw_evm_envelopes_are_not_mempool_transactions` | `agora_submitOvlExecution` rejects version 2 | `docs/core/ovl-evm.md` | NOT IMPLEMENTED |
| Native DRC↔OVL exchange | n/a | No atomic exchange | none | none | this file | NOT IMPLEMENTED |
| Bridge | n/a | Design note only | none | none | `docs/core/ovl-evm.md` | NOT IMPLEMENTED |
| Production chain id | EIP-155 | Dev 74000 and testnet 74001 are allocated. Production requires ceremony freeze | chain-id rejection test | `eth_chainId` | profile | NOT IMPLEMENTED |
