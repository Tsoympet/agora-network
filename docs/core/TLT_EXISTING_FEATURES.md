# Existing TLT features

Inventory of Talanton behavior that already existed on `cursor/drc-ledger-object-index-cdcf` before the covenant library. Nothing in this list was removed.

| Area | Existing behavior | Where |
| --- | --- | --- |
| Ledger | UTXO set in `cf_utxo`, journals in `cf_warm` | `state-machine` `apply.rs`, `utxo.rs` |
| Balances | Sum of unspent outputs per address | `balance_of` |
| Transactions | v1 version, inputs, outputs, nonce, secp256k1 pubkey and signature | `types/transaction.rs` |
| Transaction id | SHA-256 of borsh bytes | `Transaction::tx_id` |
| Signatures | Domain `agora-tx-v1`, optional chain id and genesis | `crypto/transaction.rs` |
| Issuance | Genesis premine plus coinbase only | `genesis.rs`, `apply_block` |
| Mining template | Coinbase = subsidy + fees, up to 128 mempool txs, RandomX epoch | node admit, `agora_getBlockTemplate` |
| PoW | RandomX by blue-score epoch; kHeavyHash for stratum/dev | `consensus/pow.rs` |
| Work | `work_from_bits`, GHOSTDAG `blue_work` | `daa.rs`, `ghostdag.rs` |
| Ordering | GHOSTDAG, not longest chain | `ghostdag.rs` |
| Finality | PoW threshold and independent ≥⅔ OVL and ≥⅔ DRC | `finality.rs`, `quorum.rs` |
| Difficulty | Integer DAA on the selected-parent spine, leading-zero `bits` | `daa.rs` |
| Subsidy | 50 TLT, halving every 210,000 blue score, cap 100,000,000 | `emission.rs`, `monetary.rs` |
| Premine | 10,000,000 TLT | `SupplyCaps` |
| Maturity | 100 blue score, premine exempt | `ConsensusLimits` |
| Fees | Implicit input − output, paid to the miner | `sum_transfer_fees` |
| Mempool | Signature, conflict, fee order, eviction, orphan blocks | `p2p/mempool.rs`, `ibd.rs` |
| P2P | libp2p, Agora fingerprint, IBD, compact blocks | `p2p` |
| Addresses | Bech32m `agora` / `agoratest` / `agoradev` | `hrp.rs` |
| HD wallet | BIP-39 and BIP-44 coin type 8888 | `crypto/bip44.rs`, light-client wallet |
| Coin selection | Largest-first in the TypeScript wallet (now also a bounded exhaustive selector) | `wallet.ts` |
| RPC / explorer | Balance, UTXOs, template, submit, supply, finality | `docs/core/rpc.md`, `apps/explorer` |
| State root | Sorted UTXO commitment `utxo-v1` composed with OVL and DRC | `state_root.rs` |
| Tx index | Every inclusion plus a preferred location | `tx_index.rs` |
| Supply RPC | Per-asset max, issued, burned, net | `supply.rs` |

Preserved on purpose where a Bitcoin-shaped feature would collide:

- Script-locked outputs are not the live UTXO encoding.
- SHA-256d is not the public PoW.
- Linear most-work chain selection is not the consensus rule.
- Bitcoin address prefixes, magic, and genesis are not used.
- TLT is not merged into OVL or DRC.
