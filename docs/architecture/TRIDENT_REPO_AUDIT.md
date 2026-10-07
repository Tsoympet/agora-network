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
| `TRIDENT_PROTOCOL_VERSION` | 30 | Shared by `agora-p2p` fingerprint and `agora-state-machine` genesis. v30 appends Hub / Grant / Mission registration gossip |
| Datadir `SCHEMA_VERSION` | 22 | Experimental; `OVL_EVM_SCHEMA_VERSION` is also 22 |
| DRC fee-burn schema | 20 | Lifetime burned counters |
| DRC ledger-index datadir | 21 | Common live-object / receipt rebuild |
| Combined body v18 | offer create/cancel only | Empty history keeps the frozen root |
| Combined body v20 | passport wrap v19 | Empty passports stay off the empty wire; passport-only bodies write empty offer+covenant vecs |
| Combined body v21 | hub/grant/mission wrap | Empty new lanes stay off the frozen wire; community-only bodies write empty offer+covenant+passport vecs |
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
| TLT covenants v2 | Wired (Experimental) | `NetworkMessage::TltCovenant` (appended after offer cancel) | Wired; typed compact lane 28 | `agora_submitTltCovenant`, `agora_getTltCovenant` | Shared builder + desktop/mobile P2PKH send |
| OVL account transfer | Wired | `AccountTransfer` | Wired | `agora_submitAccountTransfer` | Shared builder + desktop/mobile send |
| OVL execution (intrinsic gas) | Wired | `OvlExecution` | Wired | `agora_submitOvlExecution` | Shared v1 builder (empty calldata) + desktop/mobile send |
| OVL-EVM-v1 (`revm` Shanghai) | Experimental, genesis-gated | `NetworkMessage::OvlRawExecution` (v26); not on the Agora-signed topic | Separate raw pool + process-local inbox → template | Canonical `eth_*` / `net_*` / `web3_clientVersion`; `eth_sendRawTransaction` gossips raw | Canonical `eth_*` reads; opt-in `raw-evm.ts` helper signs/sends with an explicit secp256k1 key (never the Agora vault) |
| DRC payments + XRPL-like objects | Wired (Experimental) | Typed envelopes through offer cancel | Wired; family reservations | Typed submit/get methods | Shared builders for payment, DEX, escrow, Checks, channels, tickets, regular key, signer list, deposit preauth, account policy, trust lines, issued controls |
| DRC native DEX | Wired (Experimental) | `DrcOfferCreate` / `DrcOfferCancel` | Wired | Create/cancel + offer/account/book reads | Shared builders + desktop/mobile native-DRC-vs-issued send |
| Dual-PoS finality / staking | Wired (Experimental) | `CheckpointAttestation`, `StakeTx` | Wired | Validator / pool / `agora_submitStakeTx` | Light-client reads exist |
| Protocol treasuries / community registry | Passport + Hub/Grant/Mission + treasury spend: Wired (Experimental) | `PassportAttestation` (v29), Hub/Grant/Mission (v30), `TreasuryDisbursement` (v31) | Passport 30; hub/grant/mission 31–33; treasury 34 | Passport + hub/grant/mission + treasury submit/get/nonce; `agora_getCommunityRegistry` / `agora_getProtocolTreasuries` | Shared builders + submit. No unsigned mutation RPC |
| DA commitments | Apply + journal + TLT fee | `NetworkMessage::DataCommitment` (v25) | Wired; default Experimental boot + `DA_INCLUSION_FEE_TLT` | `agora_submitDataCommitment` / `agora_getDataCommitment` | Light-client query + submit wrappers |

`agora_getBalance` remains the TLT UTXO sum. Native OVL/DRC account
balances and shared nonces are `agora_getAccountBalances`. Ethereum
`eth_getBalance` is OVL-EVM wei and is a different ledger.

---

## P2P and compact blocks

`NetworkMessage` Borsh discriminants are append-only through
`TreasuryDisbursement` (41). Hub/Grant/Mission use 38–40; treasury uses 41.

`compact_from_block` keeps UTXO-only `CompactBlock`. Named typed lanes use
`TypedCompactBlock` with a per-kind short-id list so offers, covenants, DA,
detached DRC multisign attachments, and passport attestations cannot enter
UTXO short ids. Attachments are indexed in the mempool (`body_commitment_id`)
and gossiped on the tx topic before compact. Passport short ids use the
attestation id. Reconstruction miss or unknown kind falls back to GetBlock.
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
finality/validator/supply/treasury/registry, passport attestation and issuer
nonce reads, constitution board reads.
Ethereum JSON-RPC reads are public; `eth_sendRawTransaction` requires a
token.

Wallet-sensitive `agora_getBalance` / `agora_getAccountBalances` /
`agora_getUtxos` stay token-gated.

There is no `agora_submitDrcExecution` or generic `agora_submitExecution`.

---

## Clients

| Surface | What it does | Honest gap |
| --- | --- | --- |
| `apps/shared/light-client` | Tip sync, TLT coinselect/Merkle, vault, `sendTransfer`, typed-lane builders for every admitted DRC family plus TLT covenant P2PKH, OVL transfer/execution v1, signed passport attestations, and signed Hub/Grant/Mission registrations, Trident light-finality helper, native three-asset balance query, TLT covenant + DRC DEX/object reads, DA get/submit wrappers, canonical `eth_*` reads, opt-in raw-EVM signer | Keys stay on device. No RandomX recompute. Raw EVM uses an explicit key, not the mnemonic vault |
| Desktop / mobile wallets | TLT UTXO send, TLT covenant P2PKH, OVL transfer/execution v1, DRC payment v4, DRC offer create/cancel, DRC ticket create via shared `DRC_FAMILY_SENDERS` | No DEX book browser. Offer create is native DRC vs one issued asset. Remaining DRC families are library-complete, not per-family screens |
| Explorer | DAG, tx lookup, protocol-lane reads, mempool, node, governance panel | No DEX book order-entry UI |
| `agora-layers` HTTP | Historical lab; loopback | Non-canonical; mixed unauthenticated mutations |

Light clients stay light: they call JSON-RPC; they do not embed
infrastructure servers or store operator keys.

---

## Community, governance, treasuries

| Component | Maturity | Wiring |
| --- | --- | --- |
| Civic constitution / forum / Ecclesia prototype | Experimental administrative RPC | Local snapshot; not a consensus community lane |
| Canonical Hub registry | Experimental | Signed `HubRegistration` lane: apply/journal/gossip/mempool/RPC/light-client. First coordinator signs; apply writes Active hub |
| Canonical Passport attestations | Experimental | Signed `PassportAttestation` lane: apply/journal/gossip/mempool/RPC/light-client |
| Canonical Grant / Mission registry | Experimental | Signed `GrantRegistration` / `MissionRegistration` lanes. Registrar/sponsor must be an active hub coordinator |
| Protocol treasuries | Experimental | Signed `TreasuryDisbursement` lane: controller + authorization_root + nonce. Debits existing treasury only (no mint). TLT creates a UTXO; OVL/DRC credits the account |
| Merchant / Passport / Grants docs | Scaffold | Specs. Passport consensus is the attestation lane above, not merchant UI |

Community Definition of Done remains **INCOMPLETE**. On-chain state is
limited to balances, txs, governance, treasuries, and justified
attestations. Infrastructure (indexers, relays, sequencers) stays off
the device.

---

## Data availability

`Block.data_commitments`, atomic apply/journal, mempool reservation,
`NetworkMessage::DataCommitment` gossip, template selection, and
`agora_submitDataCommitment` / `agora_getDataCommitment` exist. Default
Experimental boot binds the DA fingerprint to the live mesh. Accepted
authorizations burn `DA_INCLUSION_FEE_TLT` (0.01 TLT) from the operator's
P2PKH UTXOs and increment TLT burned supply. Submit/apply fail without
fingerprint or without funds. Get distinguishes pending / accepted /
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
- Freeze-ready v3 artifacts (not the public draft) can be materialized into
  live Block 0 UTXO/account/treasury/validator state. `compose_trident_state_root`
  must equal the live `TridentHeader` state root before commit.
  `AGORA_TRIDENT_GENESIS_FILE` boots that datadir; `AGORA_GENESIS_FILE` stays
  v2-only. Docker-compose still points at frozen v2.
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

1. **Public Trident testnet freeze** — the checked-in v3 draft is UNFROZEN.
   Ceremony must supply allocations, validator keys, hashes, timestamp, bits,
   and a Block 0 nonce when bits are nonzero. Until that artifact exists and
   boot+IBD+tx+finality are demonstrated on a public mesh, maturity stays
   below Public testnet. Frozen v2 TLT peers boot and send, but dual-PoS
   never finalizes (empty OVL/DRC genesis sets).
2. **Civic votes** — local-admin snapshots, not consensus. Forum/Ecclesia
   RPC stays off the gossip mesh because those types are not
   network-bound secp256k1 envelopes.
3. **Vesting unlock** — genesis vesting is withheld from liquid balances,
   but there is no consensus unlock tx. Signed treasury disbursement is
   wired (v31); it does not unlock vesting.
4. **`TridentHeader` is not the gossip header** — Block 0 identity is bound
   in Meta; IBD/mining still use `BlockHeader`.

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
  reservation, template selection on default Experimental boot, TLT
  inclusion-fee debit (`DA_INCLUSION_FEE_TLT`), and
  `agora_submitDataCommitment` / `agora_getDataCommitment` with honest
  pending/accepted/confirmed/finalized statuses.
- Device-local typed-lane builders (`typed-lanes.ts` + `typed-lanes-drc.ts`)
  for DRC payment v4, DRC offer create/cancel, remaining DRC families
  (escrow, Checks, payment channels, tickets, regular key, signer list,
  deposit preauth, account policy, trust lines, issued controls), TLT
  covenant P2PKH, OVL account transfer, and Agora-signed OVL execution v1.
  Desktop/mobile submit those envelopes without embedding a node. Raw EVM
  signing lives in `raw-evm.ts` with an explicit secp256k1 key and is not
  mixed into the Agora mnemonic vault. Passport attestations use
  `typed-lanes-passport.ts`.
- `agora-node schema report|migrate|reindex` for supported library
  rebuilds. Civic write RPCs stay local-admin.
- Protocol v26 `NetworkMessage::OvlRawExecution` gossip and a separate
  raw mempool. Version 2 remains rejected by the Agora-signed pool.
- Protocol v27 `NetworkMessage::TypedCompactBlock` named-lane compact
  gossip. UTXO `CompactBlock` is unchanged.
- Protocol v28 `NetworkMessage::DrcMultisignAttachment` plus mempool
  `body_commitment_id` index and compact lane 29. Miss still GetBlock.
- Protocol v29 `NetworkMessage::PassportAttestation` (discriminant 37),
  compact lane 30, mempool issuer reservation, apply/journal revert,
  and `agora_submitPassportAttestation` / `agora_getPassportAttestation`
  / `agora_getPassportIssuerNonce`.
- Protocol v30 `NetworkMessage::HubRegistration` / `GrantRegistration` /
  `MissionRegistration` (discriminants 38–40), compact lanes 31–33, body
  wrap v21, dedicated coordinator/registrar/sponsor nonces, apply/journal
  revert, submit/get/nonce RPC, and `typed-lanes-community.ts`. No
  unsigned mutation RPC.
- Freeze-ready-only live Block 0 materializer: TLT UTXOs, OVL/DRC liquid
  accounts (allocation − vesting − self-bond), artifact treasuries/controls,
  epoch-zero validators, `compose_trident_state_root` vs live
  `TridentHeader.state_root`, `AGORA_TRIDENT_GENESIS_FILE` boot, and
  `agora-node genesis trident materialize`. Public draft stays UNFROZEN.
