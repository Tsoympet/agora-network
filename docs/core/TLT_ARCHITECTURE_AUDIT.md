# TLT architecture audit

**Status line:** `TALANTON BITCOIN FUNCTIONAL PARITY — INCOMPLETE`

**Maturity:** Live TLT UTXO, RandomX, and GHOSTDAG path is **Multi-node devnet** (testnet genesis v2 is frozen in-repo; mainnet is not bootable). Covenant programs are **Experimental** and are accepted on the block covenant lane. Overall Bitcoin-class parity stays **INCOMPLETE**.

**Base inspected:** `cursor/drc-ledger-object-index-cdcf` (PR #149). No OVL/EVM branch was stacked on that head during this audit. This work does not edit the OVL execution lane.

Talanton (TLT) is the native UTXO / PoW / value-storage asset of one Agora Trident BlockDAG L1. It is not a Bitcoin clone, not a sidechain, and not an account or contract balance. OVL remains the programmable asset. DRC remains the contract-free payment asset.

## Ledger

The spendable set is RocksDB column family `cf_utxo`, keyed by `tx_id ‖ index` (36 bytes) and valued by borsh `TxOut { value, address }`. `cf_utxo` is not pruned. Live outputs follow GHOSTDAG blues of `order_past(virtual_tip)`, not every DAG tip. Reorgs replay `utxo_diff/` journals in `cf_warm`.

Balances are the sum of unspent outputs for a 20-byte address (`balance_of`). There is no account nonce on the UTXO lane. The transaction `nonce` is a replay-binding field inside the signature, not a Bitcoin sequence number.

**Conflict kept:** Bitcoin's UTXO value is a scriptPubKey. TLT v1 outputs are address locks. Covenant P2PKH-style outputs are stored as those same `TxOut` bytes. Every other script is stored beside the UTXO set and is not part of `balance_of`.

## Balances and supply accounting

`issued_supply(TLT) <= max_supply` is enforced when coinbase emission is clamped. Genesis records premine as issued TLT. `agora_getNativeAssetSupply` reads maximum, issued, burned, and net figures for TLT, OVL, and DRC separately. TLT miner fees are not burned. DRC fee burn does not change the TLT coinbase.

## Transactions

v1 `Transaction` fields: `version`, inputs (outpoint only), outputs (`value`, `address`), `nonce`, 33-byte pubkey, 64-byte signature. The id is SHA-256 of the borsh encoding. Signatures cover `agora-tx-v1` plus optional chain id and genesis (`signing_bytes_bound`). A unit test locks the v1 byte layout so locktime and sequence stay off that wire.

v1 still has no on-wire locktime or sequence. `TltCovenantTx` carries sequence, locktime, scriptSig, and scriptPubKey on the `tlt_covenants` lane. See `docs/core/tlt-script.md`.

## Issuance

Only the genesis premine and later TLT coinbase outputs create TLT. `apply_block` requires exactly one coinbase (empty inputs). Its outputs must total at most `emission + v1 transfer fees + covenant fees`. No RPC, contract, or governance path may mint TLT on mainnet. `agora_fundAddress` is a dev/testnet faucet and is permanently disabled on mainnet.

## Mining, PoW, rewards

TLT is the only mineable asset. Admission uses RandomX (`RandomXPowHasher`) keyed by blue-score epoch (`blue_score / 2048`, domain `agora-randomx-epoch-v2`). Without the `randomx` feature, the hasher falls back to SHA-256 so the workspace still builds. That fallback is not a public-network PoW. kHeavyHash remains a stratum/dev algorithm and is not a silent public-network fallback.

`header.bits` is a leading-zero requirement. Block work is `work_from_bits`. GHOSTDAG accumulates `blue_work` (selected parent plus blue mergeset plus this block). DAA samples use that cumulative work along the selected-parent spine. Tip choice is highest blue work, then blue score, then hash. Ordering stays GHOSTDAG. It is not a linear most-work chain.

Finality is additive: PoW work threshold **and** ≥⅔ OVL stake **and** ≥⅔ DRC stake. `pow_work_met` is a depth/work threshold (`FinalityPowPolicy::min_pow_depth`). It does not replace GHOSTDAG and it does not mix stake prices.

Coinbase value is the scheduled subsidy plus accepted TLT transfer fees. Default schedule: 50 TLT (`5_000_000_000` base units) halving every `210_000` blue-score units, clamped by the 100,000,000 TLT cap. Coinbase maturity is 100 blue score. Genesis premine is exempt. These numbers already exist in `EmissionSchedule` and `ConsensusLimits`. This audit does not retune them.

## Wallet

BIP-39 mnemonics and BIP-44 `m/44'/8888'/account'/change/index` (provisional coin type 8888). Addresses are Bech32m with HRP `agora`, `agoratest`, or `agoradev`. The payload is the first 20 bytes of SHA-256(compressed secp256k1 pubkey). Desktop and mobile wallets build v1 transfers. Coin selection in `select_tlt_coins` / `selectTltCoins` prefers less change, then fewer inputs. The previous client path was largest-first only.

## APIs, explorer, SDK

Node RPC exposes balance, UTXO list, submit, fee estimate, block template (`block` plus `randomx_epoch`), submit block, transaction lookup, DAG tips, finality, and native supply. The explorer and `@agora/shared` light client read those methods. They show live node data, not a fixture chain.

Merkle proofs for the pairwise tx-id root are available to light clients. For UTXO-only blocks that root is `header.tx_root`. Multi-lane bodies wrap it, so a header root check is optional and must match only when the caller knows the block is UTXO-only.

## Networking

P2P is libp2p (gossipsub, identify, request/response). Mesh isolation is an Agora fingerprint over chain id, genesis, and consensus policy. There is no Bitcoin magic, port, or genesis. Headers-first IBD, compact blocks, and a durable orphan pool already exist. TLT mempool admission checks signatures, reserved outpoints, fees, and size limits. Fee-ordered eviction runs when the pool is full. Replace-by-fee is an explicit relay opt-in because v1 bytes cannot carry a sequence signal. It does not change which transactions are consensus-valid.

## Consensus and blocks

Blocks have multiple parents. GHOSTDAG `k` colors blue/red. The UTXO lane is coinbase plus transfers. OVL and DRC lanes are separate fields and do not spend TLT outputs. Body roots are versioned so empty later lanes do not change frozen encodings.

## Storage

Column families: hot, warm (tx index, journals, headers), archival (optional at genesis), meta, utxo. UTXO snapshots export the sorted set and the existing `utxo-v1` commitment used by the Trident state root. Archival bodies can be omitted (`write_archival: false`). The UTXO set itself is not pruned, because spends require it.

## Security

secp256k1 only, via the `secp256k1` crate. No custom elliptic curves. Domain-separated signatures. Supply clamp, coinbase maturity, structural limits, and mempool double-spend checks are in admission. Covenant scripts are bounded and are not consensus-activated.

## Monetary policy

See `TLT_MONETARY_POLICY.md`. Cap 100,000,000 TLT. Premine 10,000,000 TLT. Subsidy 50 TLT, halving interval 210,000 blue score. Not Bitcoin's 21,000,000 cap.

## Special features kept

- GHOSTDAG blue work and virtual-order conflict resolution (first blue spend wins).
- Dual independent PoS quorums beside PoW.
- Address locks and a single-key transaction signature on v1.
- Transaction nonce and chain-id binding.
- RandomX instead of SHA-256d.
- Bech32m Agora HRPs instead of base58 Bitcoin prefixes.

## Docs and tests

Module notes live under `docs/core/` (`consensus.md`, `state-machine.md`, `p2p.md`, `rpc.md`, this audit). Executable coverage includes emission halving, blue work, DAA, finality quorums, mempool admission and eviction, UTXO apply/revert, signing vectors, and the new covenant, Merkle, coin-selection, snapshot, and light-check tests. Local release timings for the script interpreter, coin selection, and Merkle proofs are in `TLT_PERFORMANCE_REPORT.md`.

## Not done

- Covenant scripts are not accepted in blocks.
- v1 transactions still have no on-wire locktime or sequence.
- Full header-chain SPV with RandomX and validator signature checks inside the light client is not a new client. Nodes already sync headers. The new helper checks a supplied Merkle proof and quorum totals.
- No TLT↔OVL/DRC wrap bridge and no exchange.
- No claim of production readiness or Bitcoin-level completeness.
