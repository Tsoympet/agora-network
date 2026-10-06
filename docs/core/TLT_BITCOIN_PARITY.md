# TLT Bitcoin functional parity

**Status line:** `TALANTON BITCOIN FUNCTIONAL PARITY — INCOMPLETE`

Highest status used for new consensus-library code is **TESTED** after the executable checks in this change. Nothing here is **PRODUCTION READY**. Live rules that already had passing tests stay marked **TESTED**. Features that exist only as an unactivated library say so in the implementation column.

| FEATURE | BITCOIN REFERENCE | TLT IMPLEMENTATION | TEST | STATUS |
| --- | --- | --- | --- | --- |
| Native UTXO accounting | UTXO set | `cf_utxo` address outputs, journals, balance sum | `apply_transfer_spends_premine_and_reverts`, `validate_mempool_tx_accepts_premine_spend` | TESTED |
| UTXO commitment / snapshot | AssumeUTXO-style set hash | Sorted `utxo-v1` export and bind check. UTXO CF is not pruned | `utxo_snapshot_binds_to_the_same_commitment` | TESTED |
| Pruning | Block file pruning, UTXO kept | Archival genesis write can be skipped. UTXO and warm journals stay | genesis `write_archival` | IMPLEMENTED |
| Coin selection | Bitcoin Core coin selection | Bounded exhaustive search, then largest-first. Wallet policy only | `prefers_exact_change_over_largest_first`, `tlt-bitcoin.test.ts` | TESTED |
| Transaction version | `nVersion` | v1 field signed in the body. Covenant library uses version 2 | v1 layout test, covenant id test | TESTED |
| Locktime | `nLockTime` | Implicit 0 on v1 wire. `lock_time` on `TltCovenantTx` only | `absolute_blue_score_lock_and_relative_sequence_lock` | TESTED |
| Sequence | `nSequence` | Implicit final on v1. Per-input sequence on covenant inputs | v1 layout test, CSV test | TESTED |
| Deterministic tx id | txid | SHA-256 of borsh. v1 bytes unchanged. Covenant id includes scriptSig; sighash omits it | v1 layout test, `covenant_id_includes_script_sig_and_sighash_does_not` | TESTED |
| P2PKH-style | HASH160 P2PKH | Agora pubkey hash (SHA-256 prefix) plus CHECKSIG. Library only | `p2pkh_style_accepts_matching_pubkey`, `secp256k1_p2pkh_style_covenant_spend` | TESTED |
| P2SH | HASH160 redeem | SHA-256 redeem script, push-only scriptSig. Library only | `p2sh_wraps_p2pkh_style_program` | TESTED |
| M-of-N | CHECKMULTISIG | M-of-N, pubkey order, no dummy element. Library only | `multisig_requires_m_of_n_in_pubkey_order` | TESTED |
| Absolute timelock | CLTV | Blue score or unix seconds. Library only | locktime test | TESTED |
| Relative timelock | CSV / BIP68 | Sequence flags and 512-second steps. Library only | sequence test | TESTED |
| Hashlock / HTLC | Script hashlock | HTLC claim or refund. No exchange and no wrap bridge | `htlc_claim_and_refund_paths` | TESTED |
| Script limits | Bitcoin script | Closed opcode set, no loops. Not block-activated | `op_return_unknown_opcode_and_unbalanced_if_fail` | TESTED |
| RandomX PoW | SHA-256d | RandomX epoch from blue score. SHA-256 hasher is a compile fallback, not public PoW | `consensus` pow tests | TESTED |
| kHeavyHash | n/a | Dev/stratum only | pow algorithm enum | IMPLEMENTED |
| Mining template | getblocktemplate | Coinbase, fees, tx root, RandomX epoch | node template path | IMPLEMENTED |
| Coinbase and fees | Subsidy + fees | Emission plus TLT transfer fees to the miner | `sum_transfer_fees`, apply tests | TESTED |
| Merkle root | tx Merkle | Pairwise tx-id root, odd leaf duplicated | `root_matches_block_tx_root_and_proofs_roundtrip` | TESTED |
| Difficulty | nBits retarget | Integer DAA, leading-zero bits, cumulative `blue_work` samples | `daa` tests, `blue_work_includes_mergeset_blues` | TESTED |
| Emission / halving | 50 BTC / 210000 / 21M | Existing 50 TLT / 210000 blue score / 100M cap. Not retuned | `halves_on_interval` | TESTED |
| Coinbase maturity | 100 blocks | 100 blue score, premine exempt | admit `check_coinbase_maturity` | IMPLEMENTED |
| Supply enforcement | 21M cap | Clamp to 100M TLT | supply + emission clamp | IMPLEMENTED |
| Mempool validation | policy checks | UTXO, signature, size, fee, conflicts | `validate_mempool_tx_*`, `admits_valid_signed_tx` | TESTED |
| Fee priority | fee rate | Fee then tx id. Evict lowest when full | `pending_entries_fee_ordered`, `full_pool_evicts_lower_fee_for_higher` | TESTED |
| RBF | BIP125 sequence | Explicit opt-in relay flag. v1 cannot signal. Consensus unchanged | `replace_by_fee_requires_opt_in_and_a_higher_fee`, descendant test | TESTED |
| Orphans | orphan pool | Block orphan pool and restart restore | `orphan_pool_parks_and_releases` | TESTED |
| Conflicts | first-seen / RBF | Consensus: first blue spend wins. Mempool: reject or opted-in replace | virtual apply, RBF tests | TESTED |
| P2P identity | network magic | libp2p Agora fingerprint. No Bitcoin magic | `fingerprint_changes_with_policy` | TESTED |
| IBD and headers | headers-first | GetHeaders, compact blocks, UTXO follow of blues | p2p ibd tests | TESTED |
| Restart / recovery | peers, mempool | Durable orphan bodies, datadir identity | orphan restore test | TESTED |
| Addresses | base58 / bech32 bc | Bech32m `agora` / `agoratest` / `agoradev` | hrp tests | TESTED |
| HD / mnemonic | BIP32 / BIP39 | BIP39 + BIP44 coin type 8888, secp256k1 | `wallet_derivation_and_tx_sign_verify_roundtrip` | TESTED |
| Explorer / API | REST | RPC balance, UTXO, template, supply, finality; explorer reads the node | rpc dispatch | IMPLEMENTED |
| Merkle proofs | `gettxoutproof` | `prove_tlt_tx_merkle` / TypeScript verifier | Rust Merkle test, `tlt-bitcoin.test.ts` | TESTED |
| SPV headers | Bitcoin SPV | Node header sync exists. Light check verifies inclusion plus PoW and both quorums. It does not redo RandomX inside the light client | `finalized_only_when_pow_and_both_quorums_agree` | TESTED |
| Multisig / timelock / HTLC for swaps | Script primitives | Library primitives only. No bridge, no exchange | HTLC and multisig tests | TESTED |
| Wrapped TLT | Wrapped BTC | Not implemented. Native TLT stays the UTXO asset | — | NOT IMPLEMENTED |
| Entire-network light client | TLT-only SPV would be insufficient | Merkle inclusion plus independent OVL and DRC quorum math | `verifyTridentLight` vector | TESTED |
| Linear most-work chain | Bitcoin main chain | Not used. GHOSTDAG plus `blue_work` kept | ghostdag tests | TESTED |
| Full audit / mainnet | Production | Not claimed | — | NOT IMPLEMENTED |

## Conflicts recorded, Agora behavior kept

- v1 address locks stay. Scripts are a side library until a later activation that does not rewrite history.
- RandomX stays the public PoW.
- GHOSTDAG stays the order. Cumulative work is the PoW threshold input and the DAA sample, not a replacement chain rule.
- DRC and OVL lanes are untouched.
- No wrap bridge in this pass.
