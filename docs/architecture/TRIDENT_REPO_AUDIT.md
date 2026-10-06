# Trident repository audit

**Maturity:** Scaffold (snapshot of live code and docs; not a production audit).  
**Status line:** `TRIDENT L1 FUNCTIONAL COMPLETENESS — INCOMPLETE`  
**Not** public-testnet ready. **Not** mainnet ready.

This document is a whole-repo inventory of what is wired on the current
three-lane BlockDAG branch versus what remains unfinished, unwired, or
honestly incomplete. It does not claim XRPL, Ethereum, or Bitcoin
equivalence.

Canonical design remains [`TRIDENT_L1.md`](TRIDENT_L1.md) and
[`TRIDENT_PHASE0_AUDIT.md`](TRIDENT_PHASE0_AUDIT.md). Asset-specific
parity files keep their own `INCOMPLETE` status lines.

---

## Snapshot constants

| Constant | Value | Notes |
| --- | --- | --- |
| `TRIDENT_PROTOCOL_VERSION` | 27 | Shared by `agora-p2p` fingerprint and `agora-state-machine` genesis. v27 appends `NetworkMessage::TypedCompactBlock` |
| Datadir `SCHEMA_VERSION` | 22 | Experimental; `OVL_EVM_SCHEMA_VERSION` is also 22 |
| DRC fee-burn schema | 20 | Lifetime burned counters |
| DRC ledger-index datadir | 21 | Common live-object / receipt rebuild |
| Combined body v18 | offer create/cancel only | Empty history keeps the frozen root |
| Combined body v19 | TLT covenants wrap v18 | Empty covenants stay off the empty wire |
| DA body wrap | v5 | Empty `data_commitments` keep the v4 root |
| Consensus / tx signing | secp256k1 | No custom crypto |
| Outer encoding | Borsh | JSON-RPC is a convenience surface |

---

## What is live on one BlockDAG

Finality is still **TLT PoW work threshold ∧ ≥⅔ OVL stake ∧ ≥⅔ DRC stake**.
Independent quorums; no price-oracle mixing.

| Lane | Consensus | Gossip | Mempool / template | RPC | Client |
| --- | --- | --- | --- | --- | --- |
| TLT UTXO + RandomX | Wired | `NetworkMessage::Transaction` / `Block` / compact | Wired | `agora_submitTransaction`, `agora_getBalance` (UTXO only), `agora_getUtxos` | Desktop / mobile send + explorer |
| TLT covenants v2 | Wired (Experimental) | `NetworkMessage::TltCovenant` (appended after offer cancel) | Wired; compact falls back to full body | `agora_submitTltCovenant`, `agora_getTltCovenant` | Shared builder + desktop/mobile P2PKH send |
| OVL account transfer | Wired | `AccountTransfer` | Wired | `agora_submitAccountTransfer` | Shared builder + desktop/mobile send |
| OVL execution (intrinsic gas) | Wired | `OvlExecution` | Wired | `agora_submitOvlExecution` | Shared v1 builder (empty calldata) + desktop/mobile send |
| OVL-EVM-v1 (`revm` Shanghai) | Experimental, genesis-gated | `NetworkMessage::OvlRawExecution` (v26); not on the Agora-signed topic | Separate raw pool + process-local inbox → template | Canonical `eth_*` / `net_*` / `web3_clientVersion`; `eth_sendRawTransaction` gossips raw | Canonical `eth_*` reads; no raw-EVM wallet builder |
| DRC payments + XRPL-like objects | Wired (Experimental) | Typed envelopes through offer cancel | Wired; family reservations | Typed submit/get methods | Shared v4 payment builder + desktop/mobile send |
| DRC native DEX | Wired (Experimental) | `DrcOfferCreate` / `DrcOfferCancel` | Wired | Create/cancel + offer/account/book reads | Shared builders + desktop/mobile native-DRC-vs-issued send |
| Dual-PoS finality / staking | Wired (Experimental) | `CheckpointAttestation`, `StakeTx` | Wired | Validator / pool / `agora_submitStakeTx` | Light-client reads exist |
| Protocol treasuries / community registry | Genesis + library | No mutation gossip | No block lane | Read RPCs | Light-client reads exist |
| DA commitments | Apply + journal | `NetworkMessage::DataCommitment` (v25) | Wired; template only when fingerprint is set | `agora_submitDataCommitment` / `agora_getDataCommitment` | Light-client query + submit wrappers |

`agora_getBalance` remains the TLT UTXO sum. Native OVL/DRC account
balances and shared nonces are `agora_getAccountBalances`. Ethereum
`eth_getBalance` is OVL-EVM wei and is a different ledger.

---

## P2P and compact blocks

`NetworkMessage` Borsh discriminants are append-only through
`TypedCompactBlock` (35). There is no community mutation gossip variant.

`compact_from_block` keeps UTXO-only `CompactBlock`. Named typed lanes use
`TypedCompactBlock` with a per-kind short-id list so offers, covenants, and DA
cannot enter UTXO short ids. Detached DRC multisign attachments have no
mempool map, so `Block::typed_compact_unnamed_lanes` still forces a full
`Block` envelope. Reconstruction miss or unknown kind falls back to GetBlock.
`Block::requires_full_body_gossip` remains the gate that UTXO compact cannot
be used for typed bodies.

IBD (`GetBlock`, orphan pool, headers-first catch-up) is wired for
stored blocks. Compact misses follow the same full-body path.

---

## RPC surface

The dispatcher in `agora-rpc` is the canonical method list. Node HTTP
auth (`AGORA_RPC_TOKEN`) leaves public chain/object reads open and
gates wallet, mining, faucet, and submit paths.

**Wired and token-gated (wallet / operator):** TLT UTXO
`agora_getBalance` / `agora_getUtxos` / `agora_getAccountBalances`,
submits, mining template, faucet, civic write RPCs.

**Wired and public:** DAG/block/tx/mempool/node/fee, TLT covenant
lookup, data-commitment lookup, every implemented DRC `Get*` family method (payments through
DEX offers, objects, receipts, channels, trust lines, issued controls),
finality/validator/supply/treasury/registry, constitution board reads.
Ethereum JSON-RPC reads are public; `eth_sendRawTransaction` requires a
token.

Wallet-sensitive `agora_getBalance` / `agora_getAccountBalances` /
`agora_getUtxos` stay token-gated.

There is no `agora_submitDrcExecution` or generic `agora_submitExecution`.

---

## Clients

| Surface | What it does | Honest gap |
| --- | --- | --- |
| `apps/shared/light-client` | Tip sync, TLT coinselect/Merkle, vault, `sendTransfer`, typed-lane builders (DRC payment/offer, TLT covenant P2PKH, OVL transfer/execution v1), Trident light-finality helper, native three-asset balance query, TLT covenant + DRC DEX/object reads, DA get/submit wrappers, canonical `eth_*` reads | Keys stay on device. No RandomX recompute. No raw-EVM builder |
| Desktop / mobile wallets | TLT UTXO send, TLT covenant P2PKH, OVL transfer/execution v1, DRC payment v4, DRC offer create/cancel | No DEX book browser. Offer create is native DRC vs one issued asset |
| Explorer | DAG, tx lookup, protocol-lane reads, mempool, node, governance panel | No DEX book order-entry UI |
| `agora-layers` HTTP | Historical lab; loopback | Non-canonical; mixed unauthenticated mutations |

Light clients stay light: they call JSON-RPC; they do not embed
infrastructure servers or store operator keys.

---

## Community, governance, treasuries

| Component | Maturity | Wiring |
| --- | --- | --- |
| Civic constitution / forum / Ecclesia prototype | Experimental administrative RPC | Local snapshot; not a consensus community lane |
| Canonical Hub / Passport / Grant / Mission registry | Scaffold | Genesis records + `agora_getCommunityRegistry`; library APIs only; no signed mutation envelope, so no block lane |
| Protocol treasuries | Scaffold / Experimental reads | `agora_getProtocolTreasuries`; signed disbursement is later |
| Merchant / Passport / Grants docs | Scaffold | Specs, not consensus |

Community Definition of Done remains **INCOMPLETE**. On-chain state is
limited to balances, txs, governance, treasuries, and justified
attestations. Infrastructure (indexers, relays, sequencers) stays off
the device.

---

## Data availability

`Block.data_commitments`, atomic apply/journal, mempool reservation,
`NetworkMessage::DataCommitment` gossip, template selection, and
`agora_submitDataCommitment` / `agora_getDataCommitment` exist. Default boot
leaves the DA fingerprint unset, so submit/inclusion fail closed with
`data commitment lane disabled pending TLT base-fee policy`.
`AGORA_ENABLE_DA_LANE=1` binds the fingerprint to the live mesh
(Experimental; no TLT debit schedule). Get distinguishes pending / accepted /
confirmed (work depth) / finalized (full PoW ∧ OVL quorum ∧ DRC quorum) /
conflict_lost / reverted and never maps lab `recordDa` to finality.
See [`../core/data-availability.md`](../core/data-availability.md).

---

## Asset parity (do not upgrade these lines)

| Asset | Canonical status | Highest honest claim |
| --- | --- | --- |
| TLT | `TALANTON BITCOIN FUNCTIONAL PARITY — INCOMPLETE` | Live UTXO/RandomX/GHOSTDAG is Multi-node devnet. Covenants are Experimental. No wrapped TLT, no v1 locktime/sequence, no 5-node production audit |
| OVL | `OVOLOS ETHEREUM FUNCTIONAL PARITY — INCOMPLETE` | Shanghai-gated OVL-EVM-v1 + 100% base-fee burn is Experimental. Raw-EVM gossip (v26) is Experimental and is not Ethereum txpool equivalence |
| DRC | XRPL capability profile: Experimental / Partial | Contract-free closed operations plus native DEX. No paths, AMM, rippling, Hooks, or XRPL wire parity |

DRC never receives a VM, bytecode, Hook, or user-defined program. OVL
is the only programmable domain.

---

## Persistence, genesis, CI

- Testnet genesis v2 is frozen in-repo. Trident v3 draft remains
  **UNFROZEN**. Mainnet is not bootable.
- Schema migrations exist as library helpers. `agora-node schema
  report|migrate|reindex --data PATH` runs the supported rebuilds
  (19→20 fee-burn, 20→21 ledger index with applied order, 21→22 marker,
  object reindex). It does not freeze genesis.
- Fast CI excludes `agora-node`, `agora-consensus`, `agora-p2p`,
  miner sidecar, stratum, seeder, and faucet. Node tests with default
  RocksDB features require `librocksdb-sys` native headers.

---

## Remaining high-confidence gaps

These are real unfinished paths, not parity slogans:

1. **Community consensus lanes** — skipped. Registry remains genesis +
   library (`register_*_into`). There is no signed Hub/Grant/Mission
   envelope to gossip without inventing a governance spend path.
2. **DA TLT inclusion-fee policy** — transport is wired; default boot
   stays fail-closed until a reviewed debit/sponsorship rule exists.
3. **Detached-multisign compact inflation** — those blocks still travel
   as full bodies because attachments are not in the mempool.

Intentionally out of scope (must stay unwired): DRC VM, TLT mining of
OVL/DRC, price-oracle stake mixing, silent kHeavyHash public PoW
fallback, embedding infra servers in light clients.

---

## This snapshot's wiring

This audit close-out adds:

- `agora_getAccountBalances` — TLT UTXO sum plus native OVL/DRC
  account balance and nonce, without changing `agora_getBalance`.
- Shared light-client wrappers for that method, TLT covenants, DRC
  DEX/escrow/check/ticket/trust-line reads, submit helpers, and
  canonical `eth_chainId` / `eth_blockNumber` / `eth_getBalance`.
- Public (no-token) access for every implemented DRC `Get*` family
  method, including escrow, Checks, channels, trust lines, issued
  controls, and DEX offer pages.
- Explorer protocol-lane panel and desktop/mobile native OVL/DRC
  balance display.
- `Block::requires_full_body_gossip` so compact gossip cannot forget a
  typed lane.
- Protocol v25 `NetworkMessage::DataCommitment` gossip, mempool
  reservation, template selection when the DA fingerprint is set, and
  `agora_submitDataCommitment` / `agora_getDataCommitment` with honest
  pending/accepted/confirmed/finalized statuses.
- Device-local typed-lane builders (`typed-lanes.ts`) for DRC payment v4,
  DRC offer create/cancel, TLT covenant P2PKH, OVL account transfer, and
  Agora-signed OVL execution v1. Desktop/mobile submit those envelopes
  without embedding a node. Raw EVM is not built in the wallet.
- `agora-node schema report|migrate|reindex` for supported library
  rebuilds. Community consensus mutation stays unwired until a signed
  envelope exists.
- Protocol v26 `NetworkMessage::OvlRawExecution` gossip and a separate
  raw mempool. Version 2 remains rejected by the Agora-signed pool.
- Protocol v27 `NetworkMessage::TypedCompactBlock` named-lane compact
  gossip. UTXO `CompactBlock` is unchanged. Detached multisign
  attachments still use the full-body fallback.
