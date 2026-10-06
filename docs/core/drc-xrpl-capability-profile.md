# DRC / XRPL capability profile

**Audited code baseline:** `bb3abe4` (`cursor/drc-contract-free-boundary-cdcf`)  
**Audit date:** 2026-10-05  
**Canonical DRC maturity:** Experimental  
**Purpose:** executable capability inventory and dependency-ordered verification plan

This profile compares the canonical Agora Drachma (DRC) state machine with
authoritative XRP Ledger semantics. It is a semantic comparison only. Agora
does not implement XRPL consensus, binary serialization, addresses, amendment
machinery, result-code classes, reserve constants, or API compatibility.

## Fixed Agora boundary

- One canonical Trident BlockDAG L1 holds TLT, OVL, and DRC state.
- TLT RandomX work orders blocks. Finality requires the configured PoW
  threshold **and** independent `>= 2/3` OVL stake **and** independent
  `>= 2/3` DRC stake.
- TLT is the only mined asset. DRC validators are PoS participants.
- OVL is the only programmable execution domain.
- DRC operations are closed, versioned, Borsh-serialized state transitions.
- DRC authorization uses secp256k1 only.
- Native DRC is not an `IssuedAssetId`. Issuer authorization, freeze,
  deep-freeze, and clawback can apply only to issuer liabilities.
- DRC has no bytecode, deploy/call, VM, EVM, Solidity, ERC-20, ERC-721, Hook,
  callback, script, or generic execution surface.
- The only in-scope exchange is a protocol-native order book. AMMs, path
  payments, autobridging, rippling, and NFTs are not implied by this profile.

## Authoritative XRPL references

The semantic reference is the official XRPL documentation reviewed on the
audit date:

- [Transactions and transaction cost](https://xrpl.org/docs/concepts/transactions/transaction-cost)
- [Transaction common fields, Sequence, Tickets, and multisign](https://xrpl.org/docs/references/protocol/transactions/common-fields)
- [Offers](https://xrpl.org/docs/concepts/tokens/decentralized-exchange/offers),
  [OfferCreate](https://xrpl.org/docs/references/protocol/transactions/types/offercreate),
  [OfferCancel](https://xrpl.org/docs/references/protocol/transactions/types/offercancel),
  and [book_offers](https://xrpl.org/docs/references/http-websocket-apis/public-api-methods/path-and-order-book-methods/book_offers)
- [Ledger entry types](https://xrpl.org/docs/references/protocol/ledger-data/ledger-entry-types)
  and [account_objects](https://xrpl.org/docs/references/http-websocket-apis/public-api-methods/account-methods/account_objects)
- [Authorized trust lines](https://xrpl.org/docs/concepts/tokens/fungible-tokens/authorized-trust-lines),
  [freezes](https://xrpl.org/docs/concepts/tokens/fungible-tokens/freezes),
  [Deep Freeze](https://xrpl.org/docs/concepts/tokens/fungible-tokens/deep-freeze),
  and [clawback](https://xrpl.org/docs/concepts/tokens/fungible-tokens/clawing-back-tokens)
- [Escrow](https://xrpl.org/docs/references/protocol/ledger-data/ledger-entry-types/escrow),
  [Check](https://xrpl.org/docs/references/protocol/ledger-data/ledger-entry-types/check),
  and [PayChannel](https://xrpl.org/docs/references/protocol/ledger-data/ledger-entry-types/paychannel)

XRPL destroys the signed XRP `Fee` for transactions included with `tes` or
`tec` results. Agora has no `tec` class: only `Accepted` operations mutate
state or pay a fee. The authorized adaptation is therefore to burn the exact
signed DRC fee only for `TransactionAcceptance::Accepted`.

## Executive audit result

The current v21 stack already contains substantially more than the earlier DRC
roadmap described:

- native DRC accounts, transfers, exact payments, tags, invoices, expiry,
  DepositAuth, regular keys, master-key disable, weighted multisign, and Tickets;
- native DRC escrow, Checks, payment channels, trust lines, issued transfers,
  authorization, freeze, deep-freeze, and clawback;
- typed block lanes, Borsh commitments, state roots, acceptance records,
  mempool reservation, full-block P2P/IBD, reorg journals, point-query RPCs,
  generated TypeScript bindings, and adversarial tests; and
- independent DRC staking and the Trident triple-conjunction finality gadget.

The two assumptions in the requested program are confirmed:

1. Accepted, fee-bearing DRC operations debit DRC but credit
   `stake/reward_pool/DRC`; they do **not** burn it.
2. No canonical Offer object, OfferCreate/OfferCancel transaction, order-book
   index, crossing engine, order-book RPC, or wallet integration exists.

Other material gaps are:

- no common typed ledger-object ID/index or `account_objects`-style enumeration;
- no generic DRC account balance/sequence query comparable to `account_info`;
- no canonical lookup for every typed DRC transaction and acceptance/receipt;
- generated types exist, but shared/desktop/mobile wallets cannot construct,
  sign, submit, or query most DRC operations;
- later DRC read methods are not all in the token-authenticated HTTP server's
  public read allowlist;
- no frozen Trident genesis, live v3 loader, in-place schema migration CLI, or
  invariant-verification command; and
- no DEX settlement convergence suite.

## Capability matrix

“Executable” means code is in the canonical transition path; it does not mean
public-testnet readiness or XRPL parity.

| Capability | Current executable behavior | Agora adaptation / deviation | Presence and maturity |
| --- | --- | --- | --- |
| Canonical ledger | DRC account, stake, authorization, and settlement state is applied beside TLT/OVL and composed into one state root | BlockDAG ordering and Trident roots, not XRPL ledgers or SHAMap | Executable · Experimental |
| Validator/finality | DRC validators sign the same checkpoints as OVL validators; finality requires PoW + both independent quorums | No UNL/Ripple consensus; no stake price mixing or admin bypass | Executable · Experimental |
| Native accounts | `(DRC, address)` balance and shared nonce; recipients can acquire account state through transfer | No XRP reserve/account-delete model; no public balance+sequence account query | Executable core · Experimental; RPC gap |
| Native transfer/payment | Exact amount, explicit DRC fee, secp256k1 auth, duplicate/replay checks, deterministic receipt/outbox | Full delivery only; no paths or partial payment | Executable · Experimental |
| Fee disposition | Accepted fees are credited to the DRC staking reward pool | Differs from XRP fee destruction; reward pool also receives reserve drips/slashes | Missing requested burn |
| Sequence/replay | Shared nonce across DRC account families; network-bound chain/genesis signing | `u64` Agora sequence model, not XRPL `UInt32` wire encoding | Executable · Experimental |
| Tickets | One ticket created per operation, 32-ticket cap, one-use nonce alternative, ticket-aware operation versions | No batch create, reserve, cancellation, expiry, or enumeration | Executable · Experimental / Single-node prototype |
| Regular key | One rotatable secp256k1 regular key | No Ed25519 | Executable · Experimental |
| Master disable | Master-only enable with alternate recovery required; regular/multisign clear; no-lockout enforcement | DRC-scoped typed policy, not XRPL account flags/wire | Executable · Single-node prototype |
| Multisign | Ordered weighted list, quorum, up to 32 secp256k1 signers; detached body-root-committed attachments | No XRPL multisign encoding or signer fee scaling | Executable · Single-node prototype |
| Tags/invoices | Optional source/destination tags, recipient-scoped invoice uniqueness, exact receipt queries | Agora Borsh/Bech32m; no XRPL JSON/binary parity | Executable · Experimental |
| Expiry | Signed inclusive `last_valid_blue_score` | GHOSTDAG blue score replaces `LastLedgerSequence` | Executable · Experimental |
| DepositAuth | Address grant/revoke; incoming direct payments and third-party escrow finish are gated | No credential authorization or reserve exception | Executable · Experimental |
| Escrow | Native DRC lock, blue-score finish/cancel gates, third-party submitters, exact receipt | No crypto-condition/hashlock and no issued assets | Executable · component labeled Multi-node devnet |
| Checks | Native DRC exact-value create/cash/cancel; no lock at create | No partial cash/`DeliverMin` and no issued assets | Executable · Single-node prototype |
| Payment channels | Native DRC create/fund/cumulative secp256k1 claim/close | Blue-score delays; no mutable expiration/renew; no Ed25519 | Executable · Single-node prototype |
| Trust lines | Holder limit, fixed-width currency, issuer liability, exact issue/redeem/transfer | Issuer-scoped one-sided model; no rippling, quality, transfer rate, or reserve | Executable · Single-node prototype |
| Issuer controls | Per-asset RequireAuth/global freeze/no-freeze/clawback and per-line auth/freeze/deep-freeze | Never applies to native DRC; policy is per issued asset rather than issuer account-wide | Executable · Single-node prototype |
| Offer/order-book DEX | None | Required target is typed, integer-only native DRC/issued-asset order books | Missing |
| Ledger-object directory | Per-family typed live objects and point indexes | No common object header/ID registry, owner directory, pagination, or account enumeration | Partial internals · Experimental |
| Public submission RPC | Typed submit methods exist for implemented DRC families | Agora JSON over Borsh-shaped types; no `submit` wire parity | Executable · Experimental |
| Public query RPC | Family-specific point queries and receipts exist | No `account_info`, `account_objects`, `account_offers`, `book_offers`, or all-family tx lookup | Partial · Experimental |
| Wallet/client | Generated TypeScript types exist; finality/reward-pool reads exist in shared light client | No DRC balance, typed construction/signing/submission, object query, or receipt UX | Missing |
| Mempool/template | Shared nonce/Ticket and family-specific object reservations; deterministic lane order; full-body templates | Same-block dependencies are often intentionally fail-closed in public admission | Executable · Experimental |
| P2P/IBD | Typed operation gossip and full multi-lane block relay; compact blocks fall back to full body | No XRPL peer/wire protocol | Executable · Experimental |
| Reorg/restart | Account/object/root snapshots revert atomically; RocksDB paths and family tests exist | Included typed operations generally require explicit resubmission after reorg | Executable core · Experimental |
| Genesis/migration | Draft v3 commits versions/policy; schema v19 names current key families | Draft is unfrozen, loader is disabled, and no migration/reindex/verify CLI exists | Scaffold / Experimental |
| Smart contracts | DRC selectors/payloads/generic execution routes reject; OVL execution remains separate | Permanent exclusion | Enforced |

## Current protocol profile

| Surface | Current value |
| --- | --- |
| Trident protocol | `21` |
| Transaction signing profile | `agora-trident-tx-v9` |
| State transition | `agora-trident-state-v19` |
| Highest DRC block-body wrapper | `agora-block-body-v17` |
| Composed state-root domain | `agora-trident-state-root-v13` |
| Datadir schema | `19` |
| Genesis | v3 draft, `UNFROZEN`; not bootable as a live Trident network |
| DRC exchange | none |
| DRC fee sink | staking reward pool |

The P2P fingerprint commits the protocol, signing, state-transition, consensus
policy, chain identity, and genesis identity. Any consensus fee-burn or Offer
activation must advance the applicable versions and fingerprint together.

## Cross-cutting implementation coverage

The implemented DRC families have broad but non-uniform coverage:

- central master/regular/multisign/master-disabled verification;
- nonce/Ticket selection and network-bound replay protection;
- explicit body lanes and acceptance vectors;
- a copy-on-write same-block state overlay;
- Meta/account snapshots in `UtxoJournal` and atomic RocksDB batches;
- state-root components for each current object family;
- operation gossip plus full-block fallback for non-UTXO lanes;
- family-specific mempool reservations and template tests; and
- point-query RPC/receipt tests.

Known cross-cutting limitations:

- The 1 MB block-byte limit bounds aggregate DRC work, and object families have
  selected owner/index caps, but there is no uniform per-lane operation cap.
  Offer crossing needs explicit per-operation and per-block match limits.
- `agora_getTransaction` indexes only the TLT `transactions` vector. Typed DRC
  lane operation IDs do not have one common canonical transaction lookup.
- Point-query key families are not a common owner/object directory and cannot
  support complete wallet synchronization.
- Current state-root composition commits account balances and DRC object
  roots, but there is no explicit native supply/burn component.
- `verify_supply_invariants` checks only `issued <= max`; it does not track
  lifetime burn, net issued supply, or full balance/lock/stake/pool
  conservation.

## Fee accounting decision

The high-confidence adaptation is:

```text
lifetime_burned_DRC' = lifetime_burned_DRC + accepted_signed_fee
net_issued_DRC       = issued_DRC - lifetime_burned_DRC
```

Rules:

1. Increment only after the DRC operation has passed all validation and is
   classified `Accepted`.
2. `ExactDuplicate`, `ConflictLost`, malformed, expired, unauthorized, and
   otherwise rejected operations burn zero and do not consume sequence/Ticket.
3. Preserve signed transaction bytes and fee fields.
4. Do not credit new DRC transaction fees to the reward pool.
5. Do not change TLT miner fees or OVL fee/reward behavior.
6. Keep pre-activation DRC reward-pool value intact. It may contain fees,
   staking-reserve drips, and slash proceeds, so retroactive fee extraction is
   not deterministic.
7. Commit issued, burned, and net accounting to the canonical state root;
   expose it through read-only RPC.
8. Snapshot the burned counter in the same operation batch/journal so rollback,
   reorg, and restart are exact.
9. Initialize lifetime burned to zero at the activation boundary. A future
   public-network migration must use a frozen activation height/state root;
   no public Trident v3 network currently exists to migrate.

This decision changes only fee disposition. It does not select DRC issuance,
staking reserve, validator reward, or community reward rates.

## Order-book target boundary

The next exchange transaction family must be `OfferCreate` / `OfferCancel`
style typed state transitions, but it must not mechanically copy XRPL wire
fields or reserve constants.

Required foundation:

- stable typed ledger-object IDs and owner/book indexes;
- asset amount enum distinguishing native DRC from `IssuedAssetId`;
- exact positive integer amounts with checked `u128` intermediates;
- rational quality comparison by cross multiplication, never floating point;
- deterministic price/time priority with an explicit tie-break committed in
  state;
- owner funding checks at crossing time, including issuer exemptions only
  where explicitly specified;
- trust-line authorization, global/line freeze, and deep-freeze gating;
- deterministic self-cross cancellation and partial-fill rounding;
- bounded offers per owner/book, matches per transaction, and total book work
  per block;
- OfferCreate/OfferCancel master, regular, multisign, disabled-master,
  nonce/Ticket, expiry, replay, fee-burn, mempool, template, P2P, IBD,
  reorg/restart, state-root, RPC, and wallet matrices; and
- account-offers, book-offers, object, transaction, and receipt queries with
  bounded pagination.

Explicitly excluded from that slice: path payments, autobridging, rippling,
transfer rates, AMMs, NFTs, permissioned DEXes, Hooks, and contracts.

## Detailed test plan

### Baseline and generated artifacts

1. Record `git status`, toolchain, dependency setup, current protocol constants,
   and the frozen/draft genesis status.
2. Run focused existing DRC state-machine, crypto, RPC, node admission/template,
   P2P/IBD, reorg, RocksDB reopen, and two-node tests.
3. Run `cargo test --workspace --all-targets`,
   `cargo fmt --all -- --check`, and
   `cargo clippy --workspace --all-targets -- -D warnings`.
4. Run `cargo test -p agora-types export_shared_types`, then require zero Git
   diff under `core/crates/types/bindings`.
5. Validate OpenAPI method closure and build applicable shared/explorer/desktop
   clients.

### Per-operation authorization and replay matrix

For every new transaction:

- master succeeds while enabled;
- valid regular key succeeds;
- valid weighted multisign succeeds in canonical signer order;
- disabled master fails;
- malformed, mixed single+multisign, duplicate signer, wrong owner, wrong
  operation kind, wrong chain, wrong genesis, and signature tampering fail;
- exact nonce succeeds once, stale/future nonce fails;
- live Ticket succeeds once without advancing the ordinary nonce;
- reused, missing, wrong-owner, and legacy-version Ticket selectors fail;
- malformed or cross-lane multisign attachments fail before mutation; and
- every rejection preserves balances, objects, sequence/Ticket, fee accounting,
  acceptance, and state root.

### Fee-burn matrix

- one accepted operation increments burned by exactly its signed fee;
- multiple accepted DRC families add exactly, including zero-fee fixtures;
- duplicate/conflict/invalid/expired/unauthorized operations burn zero;
- self-payment has fee-only net balance reduction and matching burn;
- DRC reward pool is unchanged by fees but still changes for slash/reserve
  sources;
- TLT coinbase fees and OVL reward-pool fees are unchanged;
- arithmetic overflow and `burned > issued` fail closed;
- issued, burned, and net values survive reopen and match RPC;
- apply→revert restores counter/root exactly; alternate reapply produces the
  alternate exact counter;
- multi-block reorg and crash/restart preserve
  `net = issued - burned`; and
- genesis and pre-activation schema handling initialize a deterministic zero
  counter without inferring historical pool provenance.

### Offer/order-book matrix

- IDs, Borsh bytes, signing preimages, body roots, and state roots have locked
  vectors;
- price comparison uses integer cross multiplication at min/max boundaries;
- best quality first and deterministic tie-break order;
- no-cross, exact fill, maker partial, taker partial, multi-maker, and dust
  boundaries;
- self-cross cancellation and OfferCancel idempotence are explicitly pinned;
- native DRC/native DRC and identical-asset books reject; supported books
  distinguish native DRC from issued assets;
- unfunded, partially funded, deleted-line, unauthorized, frozen,
  deep-frozen, globally frozen, and issuer-side cases;
- funding cannot be double-reserved by concurrent offers or other DRC lanes;
- match/offer/book/account caps fail before partial mutation;
- accepted fees burn once even when no remainder is placed;
- all authorization, Ticket, mempool, template, P2P/full-block/IBD, reorg,
  reopen, query pagination, and wallet round-trip matrices above; and
- at least two nodes receiving candidate operations in different arrival
  orders converge on identical fills, receipts, object sets, roots, and burned
  supply after canonical block order.

### Negative execution-boundary matrix

Reject DRC transaction/object/RPC/P2P fields or names containing generic
execution, bytecode, VM, deploy/call, EVM, Solidity, ERC token, NFT, Hook,
callback, script, or arbitrary program semantics. Keep the positive OVL
execution path working.

## Dependency-ordered implementation queue

1. This audited capability profile and documentation reconciliation.
2. Accepted-only DRC fee burn, lifetime counter, net-supply invariant,
   state-root/version/schema/fingerprint activation, journal/reorg/restart,
   genesis initialization, RPC, client read support, and tests.
3. Common DRC ledger-object identity/owner index and canonical typed operation
   lookup, without adding a generic mutation API.
4. Native order-book Offer objects plus OfferCreate/OfferCancel and deterministic
   crossing.
5. Public account/object/offer/book/transaction/receipt RPCs with bounded
   pagination.
6. Shared wallet typed construction/signing/query support, then desktop/mobile
   UX.
7. Multi-node deterministic settlement/convergence and adversarial load/crash
   matrices.
8. Frozen genesis activation and explicit migration/reindex/invariant tooling.

## Genuine policy blockers

- DRC genesis allocation, staking reserve, reserve drip, and any future reward
  curve remain working defaults, not ceremony-frozen policy.
- The DRC fee burn can be implemented without resolving those rates, but no
  production net-supply forecast can be claimed.
- Offer owner-reserve economics are unspecified. The safe initial adaptation is
  hard object/work caps plus normal accepted transaction fee burn; copying XRP
  reserve constants is not authorized.
- A public activation height is unavailable because Trident genesis v3 is
  unfrozen and the live loader is disabled.

No item in this profile establishes full XRPL parity, Public testnet maturity,
Audited production maturity, or Mainnet ready status.
