# TLT Bitcoin functional parity

**Status line:** `TALANTON BITCOIN FUNCTIONAL PARITY — INCOMPLETE`

Highest status used for this change is **TESTED**. Nothing here is **PRODUCTION READY**. The overall status line stays **INCOMPLETE**: wrapped TLT is unimplemented, v1 still has no on-wire locktime or sequence, the light client does not recompute RandomX, and there is no 5-node production audit.

| FEATURE | BITCOIN REFERENCE | TLT IMPLEMENTATION | TEST | STATUS |
| --- | --- | --- | --- | --- |
| Native UTXO accounting | UTXO set | `cf_utxo` address outputs, journals, balance sum | `apply_transfer_spends_premine_and_reverts`, `validate_mempool_tx_accepts_premine_spend` | TESTED |
| UTXO commitment / snapshot | AssumeUTXO-style set hash | Sorted `utxo-v1` export and bind check. UTXO CF is not pruned | `utxo_snapshot_binds_to_the_same_commitment` | TESTED |
| Pruning | Block file pruning, UTXO kept | Archival genesis write can be skipped. UTXO and warm journals stay | genesis `write_archival` | IMPLEMENTED |
| Coin selection | Bitcoin Core coin selection | Bounded exhaustive search, then largest-first. Wallet policy only | `prefers_exact_change_over_largest_first`, `tlt-bitcoin.test.ts` | TESTED |
| Transaction version | `nVersion` | v1 wire stays version 1. Covenant lane uses version 2 | v1 layout test, covenant id test | TESTED |
| Locktime | `nLockTime` | Implicit 0 on v1 wire. Covenant lane enforces blue-score and unix locktime | `cltv_csv_and_malformed_scripts_fail_closed` | TESTED |
| Sequence | `nSequence` | Implicit final on v1. Covenant inputs carry sequence, CSV, and RBF signaling | `cltv_csv_and_malformed_scripts_fail_closed`, relay test | TESTED |
| Deterministic tx id | txid | SHA-256 of borsh. v1 bytes unchanged. Covenant id includes scriptSig; sighash omits it | v1 layout test, `covenant_id_includes_script_sig_and_sighash_does_not` | TESTED |
| P2PKH-style | HASH160 P2PKH | Agora pubkey hash (SHA-256 prefix) plus CHECKSIG. Block-accepted; stored as `TxOut` | `p2pkh_covenant_spends_premine_pays_fee_reverts_and_replays` | TESTED |
| P2SH | HASH160 redeem | SHA-256 redeem script, push-only scriptSig. Block-accepted | `p2sh_multisig_and_htlc_are_block_accepted` | TESTED |
| M-of-N | CHECKMULTISIG | M-of-N, pubkey order, no dummy element. Block-accepted | `p2sh_multisig_and_htlc_are_block_accepted` | TESTED |
| Absolute timelock | CLTV | Blue score or unix seconds on the covenant lane | `cltv_csv_and_malformed_scripts_fail_closed` | TESTED |
| Relative timelock | CSV / BIP68 | Sequence flags and 512-second steps on stored script outputs | `cltv_csv_and_malformed_scripts_fail_closed` | TESTED |
| Hashlock / HTLC | Script hashlock | HTLC claim path is block-accepted. No exchange and no wrap bridge | `p2sh_multisig_and_htlc_are_block_accepted` | TESTED |
| Script limits | Bitcoin script | Closed opcode set, no loops. Malformed spends fail the block | `cltv_csv_and_malformed_scripts_fail_closed` | TESTED |
| RandomX PoW | SHA-256d | RandomX epoch from blue score. SHA-256 hasher is a compile fallback, not public PoW | `consensus` pow tests | TESTED |
| kHeavyHash | n/a | Dev/stratum only | pow algorithm enum | IMPLEMENTED |
| Mining template | getblocktemplate | Coinbase, fees, tx root, RandomX epoch | node template path | IMPLEMENTED |
| Coinbase and fees | Subsidy + fees | Emission plus v1 transfer fees plus covenant fees | `p2pkh_covenant_spends_premine_pays_fee_reverts_and_replays` | TESTED |
| Merkle root | tx Merkle | Pairwise v1 tx-id root, odd leaf duplicated. Header root wraps covenants only when that lane is non-empty | `p2pkh_covenant_spends_premine_pays_fee_reverts_and_replays` | TESTED |
| Difficulty | nBits retarget | Integer DAA, leading-zero bits, cumulative `blue_work` samples | `daa` tests, `blue_work_includes_mergeset_blues` | TESTED |
| Emission / halving | 50 BTC / 210000 / 21M | Existing 50 TLT / 210000 blue score / 100M cap. Not retuned | `halves_on_interval` | TESTED |
| Coinbase maturity | 100 blocks | 100 blue score, premine exempt | admit `check_coinbase_maturity` | IMPLEMENTED |
| Supply enforcement | 21M cap | Clamp to 100M TLT | supply + emission clamp | IMPLEMENTED |
| Mempool validation | policy checks | v1 checks unchanged. Covenant lane checks script, locktime, min relay fee, and conflicts | `tlt_covenant_submit_query_template_and_restart` | TESTED |
| Fee priority | fee rate | Fee then tx id. Evict lowest when full | `pending_entries_fee_ordered`, `full_pool_evicts_lower_fee_for_higher` | TESTED |
| RBF | BIP125 sequence | v1 stays explicit opt-in. Covenant replacement requires a non-final sequence and does not evict v1 | `covenant_relay_orders_by_fee_and_replaces_only_signaled_conflicts` | TESTED |
| Orphans | orphan pool | Block orphan pool and restart restore | `orphan_pool_parks_and_releases` | TESTED |
| Conflicts | first-seen / RBF | Consensus: first blue spend wins. Mempool: reject or opted-in replace | virtual apply, RBF tests | TESTED |
| P2P identity | network magic | libp2p Agora fingerprint. No Bitcoin magic | `fingerprint_changes_with_policy` | TESTED |
| IBD and headers | headers-first | GetHeaders, compact blocks, UTXO follow of blues | p2p ibd tests | TESTED |
| Restart / recovery | peers, mempool | Durable orphan bodies, datadir identity | orphan restore test | TESTED |
| Addresses | base58 / bech32 bc | Bech32m `agora` / `agoratest` / `agoradev` | hrp tests | TESTED |
| HD / mnemonic | BIP32 / BIP39 | BIP39 + BIP44 coin type 8888, secp256k1 | `wallet_derivation_and_tx_sign_verify_roundtrip` | TESTED |
| Explorer / API | REST | RPC balance, UTXO, template, supply, finality, plus covenant submit and query | `submit_and_query_tlt_covenant` | TESTED |
| Merkle proofs | `gettxoutproof` | `prove_tlt_tx_merkle` / TypeScript verifier | Rust Merkle test, `tlt-bitcoin.test.ts` | TESTED |
| SPV headers | Bitcoin SPV | Node header sync exists. Light check verifies inclusion plus PoW and both quorums. It does not redo RandomX inside the light client | `finalized_only_when_pow_and_both_quorums_agree` | TESTED |
| Multisig / timelock / HTLC for swaps | Script primitives | Block-accepted on the covenant lane. No bridge and no exchange | P2SH, multisig, HTLC, CLTV, CSV tests | TESTED |
| Wrapped TLT | Wrapped BTC | Not implemented. Native TLT stays the UTXO asset | — | NOT IMPLEMENTED |
| Entire-network light client | TLT-only SPV would be insufficient | Merkle inclusion plus independent OVL and DRC quorum math | `verifyTridentLight` vector | TESTED |
| Linear most-work chain | Bitcoin main chain | Not used. GHOSTDAG plus `blue_work` kept | ghostdag tests | TESTED |
| Full audit / mainnet | Production | Not claimed | — | NOT IMPLEMENTED |

## Conflicts recorded, Agora behavior kept

- v1 address locks and v1 bytes stay. Covenant spends are a separate lane and do not rewrite history.
- RandomX stays the public PoW.
- GHOSTDAG stays the order. Cumulative work is the PoW threshold input and the DAA sample, not a replacement chain rule.
- DRC and OVL lanes are untouched.
- No wrap bridge in this pass.
