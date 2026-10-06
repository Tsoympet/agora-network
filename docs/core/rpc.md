# RPC (`agora-rpc`)

Access layer for wallets, explorer, faucet, and CEX gateways.

## Methods

| Method | Purpose |
| --- | --- |
| `agora_getDagTips` | Current DAG tips (hex hashes) |
| `agora_getBlock` | Block by hash |
| `agora_getTransaction` | Lookup by `tx_id`: `pending` (mempool) / `confirmed` (indexed) / `unknown` |
| `agora_getMempool` | Pending pool snapshot (`count` + fee-ordered `transactions`, optional `limit`) |
| `agora_getNodeInfo` | Operator snapshot: peer id, connected peers, tips, mempool, PoW, archival / hot window, `min_relay_fee` |
| `agora_estimateFee` | `{ min_relay_fee, suggested_fee }` for wallet coin selection |
| `agora_submitTransaction` | UTXO-check + admit a signed tx into the mempool and gossip it |
| `agora_submitAccountTransfer` | Validate, reserve, and gossip a signed OVL/DRC account transfer |
| `agora_submitOvlExecution` | Validate and gossip a signed intrinsic-gas OVL execution envelope |
| `agora_submitDrcPayment` | Validate and gossip a versioned signed DRC payment with source/destination-tag and invoice routing |
| `agora_getDrcPayment` | Read a canonical settled DRC exact-delivery receipt by payment ID (`settled` / `unknown`) |
| `agora_getDrcPaymentByInvoice` | Resolve an exact recipient/invoice tuple to its canonical settled DRC receipt |
| `agora_submitDrcAccountPolicy` | Validate, reserve, and gossip a signed DRC destination-tag/DepositAuth policy operation |
| `agora_getDrcAccountPolicy` | Read canonical recipient policy and shared DRC nonce (`known` / `unknown`) |
| `agora_submitDrcDepositPreauth` | Validate, reserve, and gossip an owner-signed address grant/revoke |
| `agora_getDrcDepositPreauth` | Read one canonical recipient/source DepositAuth status without enumeration |
| `agora_submitDrcRegularKey` / `agora_getDrcAccountKeys` | Rotate or query the account’s regular-key authorization state |
| `agora_submitDrcSignerList` / `agora_getDrcAccountSignerList` | Set or query weighted multisign authorization |
| `agora_submitDrcTicketCreate` / `agora_getDrcTicket` | Create account-sequence tickets or query one exact ticket |
| `agora_submitDrcEscrowCreate` / `agora_submitDrcEscrowFinish` / `agora_submitDrcEscrowCancel` | Admit native DRC escrow lifecycle operations |
| `agora_getDrcEscrow` / `agora_getDrcEscrowReceipt` | Query one live/closed escrow or its receipt |
| `agora_submitDrcCheckCreate` / `agora_submitDrcCheckCash` / `agora_submitDrcCheckCancel` | Admit native DRC check lifecycle operations |
| `agora_getDrcCheck` | Point lookup: `{ check_id, status }` where `status` is `live`, `receipt`, or `unknown` |
| `agora_getDrcCheckReceipt` | Closed check receipt by `check_id` (`known` / `unknown`) |
| `agora_submitDrcPaymentChannelCreate` | Validate, virtual-apply, mempool-admit, and gossip a signed DRC payment channel create (**Experimental**; accepted ≠ finality) |
| `agora_submitDrcPaymentChannelFund` | Owner-only fund of a live channel |
| `agora_submitDrcPaymentChannelClaim` | Destination-submitted on-chain claim with off-ledger cumulative signature |
| `agora_submitDrcPaymentChannelClose` | Owner schedule/finalize or destination immediate close |
| `agora_getDrcPaymentChannel` | Point lookup: `{ channel_id, status }` — `live`, `receipt`, or `unknown` (zero id → `-32602`) |
| `agora_getDrcPaymentChannelReceipt` | Closed channel receipt (`known` / `unknown`) |
| `agora_getDrcPaymentChannelFundEvent` | Immutable fund event by `fund_tx_id` (`known` / `unknown`; no scan) |
| `agora_getDrcPaymentChannelClaimEvent` | Immutable claim event by `claim_tx_id` |
| `agora_getDrcPaymentChannelScheduleEvent` | Schedule event by close `close_tx_id` |
| `agora_verifyDrcPaymentChannelClaim` | Pure verification of supplied cumulative claim signature against a **live** channel (no private keys; no mutation) |
| `agora_submitDrcTrustLineSet` / `agora_submitDrcIssuedTransfer` | Admit contract-free trust-line or exact issued-value operations |
| `agora_getDrcTrustLine` / `agora_getDrcIssuerLiability` / `agora_getDrcIssuedTransferReceipt` | Exact trust-line, liability, and transfer-receipt queries |
| `agora_submitDrcIssuedAssetPolicySet` / `agora_submitDrcTrustLineIssuerControl` / `agora_submitDrcIssuedClawback` | Admit issued-asset authorization, freeze, or exact clawback controls |
| `agora_getDrcIssuedAssetPolicy` | Query the live policy flags for one `(issuer, currency)` asset |
| `agora_getDrcIssuedAssetPolicyReceipt` / `agora_getDrcTrustLineIssuerControlReceipt` / `agora_getDrcIssuedClawbackReceipt` | Exact issued-control receipt queries |
| `agora_submitDrcOfferCreate` / `agora_submitDrcOfferCancel` | Admit a signed native order-book create or cancel. Responses set `simulated_fill` to false |
| `agora_getDrcOffer` | Point lookup of one live offer, including currently deliverable `taker_gets_funded`, or the create receipt when the offer is absent |
| `agora_getDrcAccountOffers` | Bounded owner page. The cursor is the last offer id |
| `agora_getDrcBookOffers` | Bounded direct-book page in committed quality order. The cursor is the sort key, not a fill |
| `agora_getDrcObject` | Point lookup for one deterministic common DRC live-object ID |
| `agora_getDrcAccountObjects` | Bounded, cursor-paginated common live objects for one owner, optionally filtered by closed object kind |
| `agora_getDrcOperation` | Canonical accepted DRC operation receipt by domain-separated operation ID |
| `agora_getDrcTransaction` | Canonical accepted DRC operation receipt by the historical signed transaction ID |
| `agora_getBalance` | Address TLT UTXO balance (sum of live `cf_utxo`) |
| `agora_getAccountBalances` | TLT UTXO sum plus native OVL/DRC account balance and nonce. Missing accounts read as zeros. Does not change `agora_getBalance` and is not EVM wei |
| `agora_getUtxos` | Spendable outpoints for an address (`tx_id`, `index`, `value`) |
| `agora_fundAddress` | Dev/testnet mint: write a spendable `cf_utxo` (needs `AGORA_RPC_ALLOW_FUND`; **permanently disabled on mainnet**) |
| `agora_getBlockTemplate` | Mining template block (tips as parents + coinbase) |
| `agora_submitBlock` | Admit a mined block (PoW verify + store + gossip) |
| `agora_getFinality` | Trident checkpoint certificate / state for a block hash |
| `agora_getFinalizedTip` | Finalized blue-score frontier |
| `agora_submitAttestation` | Admit + gossip an OVL/DRC checkpoint attestation |
| `agora_getValidatorSet` | OVL/DRC validator snapshot (`asset`, optional `epoch`) |
| `agora_getValidator` | One validator record |
| `agora_getRewardPool` | Slash/reward pool balance for OVL or DRC |
| `agora_getNativeAssetSupply` | Read maximum, issued, lifetime burned, and checked net supply for TLT/OVL/DRC as decimal strings |
| `agora_getProtocolTreasuries` | Canonical governance policy/root and asset-isolated treasury balances |
| `agora_getCommunityRegistry` | Read canonical Hub, Passport, Grant, and Mission registry summary/records |
| `agora_submitStakeTx` | Validate, reserve, and gossip a secp256k1-signed stake tx for block inclusion |
| `agora_getConstitution` | Enacted constitution id, content hash, body |
| `agora_getGovernance` | Civic overview (params, offices, counts) |
| `agora_listProposals` / `agora_getProposal` | Proposal ballot board |
| `agora_listOffices` | Archon / Bouleutes / Tamias seats |
| `agora_listForumTopics` | Community board topics |
| `agora_submitProposal` | Open a proposal (deposit phase) |
| `agora_depositProposal` | Add deposit toward `min_deposit` |
| `agora_openProposalVoting` | Move deposit → voting |
| `agora_castGovVote` | Cast Yes/No/Abstain/NoWithVeto (quadratic in Ecclesia) |
| `agora_tallyProposal` / `agora_enterProposalTimelock` / `agora_executeProposal` | Lifecycle |
| `agora_sponsorProposal` / `agora_assentProposal` | Tamias sponsor / Archon assent |
| `agora_postForumTopic` / `agora_ackConstitution` | Community board + constitution ack |

## Execution boundary

`agora_submitOvlExecution` is the only Agora-signed programmable-execution
submission method. Its `OvlExecutionTx` is intrinsically OVL-denominated and has
no asset selector. Unknown fields such as `"asset": "DRC"` are rejected rather
than ignored. There is no generic `agora_submitExecution`, DRC deploy/call, DRC
VM, or contract-facing DRC method.

## OVL Ethereum JSON-RPC

`eth_*`, `net_*`, and `web3_clientVersion` are dispatched to
`RpcBackend::ovl_ethereum_rpc`. The default implementation is method-not-found.
`agora-node` loads the canonical OVL-EVM-v1 world and answers from it. Responses
use a JSON-RPC 2.0 envelope.

Public reads when `AGORA_RPC_TOKEN` is set include `web3_clientVersion`,
`net_version`, `net_listening`, `net_peerCount`, `eth_chainId`, `eth_syncing`,
`eth_blockNumber`, `eth_gasPrice`, `eth_maxPriorityFeePerGas`,
`eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt`,
`eth_call`, `eth_estimateGas`, `eth_feeHistory`, `eth_getTransactionByHash`,
`eth_getTransactionReceipt`, `eth_getBlockByNumber`, `eth_getBlockByHash`,
`eth_getLogs`, the block transaction-count methods, and the transaction-by-index
methods. `eth_sendRawTransaction` requires the token and the dev/test gate.
`eth_getProof` is not implemented. See [`ovl-evm.md`](ovl-evm.md).

DRC payment, escrow, Check, payment-channel, trust-line, issued-asset,
freeze/clawback, multisign, regular-key, and Ticket methods submit closed typed
protocol operations. Their metadata and authorization fields are inputs to
deterministic native state transitions, not bytecode or contract-call data.
This typed surface does not claim full XRPL compatibility.

Civic state is persisted under RocksDB Meta key `meta/governance` (`CivicSnapshot`).

There is no data-commitment submit or confirmation RPC. The TLT
`agora_submitTransaction` schema has no payload field, and commitment bytes
must not be hidden in a transfer or OVL execution call. See
[`data-availability.md`](data-availability.md) for the block/state consensus
lane and remaining fee, mempool, outbox, and transport prerequisites.

The internal acceptance record can distinguish `Accepted`, `ExactDuplicate`,
and `ConflictLost`, but that alone cannot truthfully report `pending` or
`orphaned` to a client. An authenticated submit/status API remains disabled
until it has durable mempool/outbox identity, virtual-view lookup, and states
separate from full PoW + dual-PoS finality.

## Dispatch

- `RpcBackend` — trait implemented by node services
- `InMemoryBackend` — ledger + tips/blocks/mempool for tests and the faucet scaffold
- `RpcDispatcher` — parses `RpcRequest`, returns `RpcResponse` with `result` or `error`

## HTTP transport (`agora-node`)

Wired in `core/node-bin`:

| Env | Default | Meaning |
| --- | --- | --- |
| `AGORA_RPC_BIND` | `127.0.0.1:8545` | HTTP JSON-RPC listen address (non-loopback requires `AGORA_RPC_ALLOW_PUBLIC_BIND=1`) |
| `AGORA_RPC_TOKEN` | unset | When set, wallet/mining/fund RPC methods require `Authorization: Bearer <token>` |
| `AGORA_RPC_ALLOW_PUBLIC_BIND` | unset | When `1`/`true`, allow binding RPC on a non-loopback address |
| `AGORA_RPC_ALLOW_FUND` | unset | When `1`/`true`, enable `agora_fundAddress` on `dev`/`testnet` only (ignored on mainnet) |
| `AGORA_RPC_RATE_LIMIT` | `120` | Max POST `/rpc` requests per peer IP per rolling minute (`0` disables) |
| `AGORA_POW_ALGO` | `randomx` | PoW algorithm (**dev override only**; testnet/mainnet use `ChainParams.pow_algorithm`) |
| `AGORA_TEMPLATE_BITS` | `1` | Initial DAA difficulty on **dev** only; frozen networks use `ChainParams.bits` |
| `AGORA_MINER_ADDRESS` | `00…00` | Coinbase payout (`agora1…` Bech32m or 40-char hex) for templates |
| `AGORA_NETWORK` | `dev` | `dev` (free genesis) / `testnet` (frozen) / `mainnet` (not frozen); also scopes P2P gossip topics |
| `AGORA_PREMINE_ADDRESS` | `00…00` | Genesis premine (**dev only**; ignored on frozen networks); fresh `AGORA_DATA` |
| `AGORA_GENESIS_FILE` | unset | Optional path to a genesis JSON artifact (`docs/genesis/*.genesis.json`) |
| `AGORA_EXPECTED_GENESIS` | unset | Extra hex Block 0 check after load/ignite |
| `AGORA_MIN_RELAY_FEE` | `1` | Minimum implicit fee (`in − out`) for mempool admission |
| `AGORA_ARCHIVAL` | `1` | Persist full block history in `cf_archival` (`0` = pruned node) |
| `AGORA_HOT_WINDOW` | `64` | Tip-distance of block bodies kept in `cf_hot` (`0` = unlimited) |

Endpoints:

- `GET /health` → `{"ok":true}` (always unauthenticated)
- `POST /` or `POST /rpc` → JSON body is an `RpcRequest`
- CORS enabled (`Access-Control-Allow-Origin: *`) for browser explorers; `OPTIONS` preflight supported (`authorization` allowed)

### Auth (`AGORA_RPC_TOKEN`)

When unset, JSON-RPC stays open (safe with the default loopback bind). When set:

| Always public | Token required |
| --- | --- |
| `GET /health` | `agora_submitTransaction` / `agora_submitBlock` |
| `agora_getDagTips` / `agora_getBlock` / `agora_getTransaction` / DRC payment, policy, and preauthorization reads | `agora_getBlockTemplate` / `agora_fundAddress` |
| `web3_clientVersion`, `net_*`, and `eth_*` reads | `eth_sendRawTransaction` |
| `agora_getMempool` / `agora_getNodeInfo` / `agora_estimateFee` | `agora_getBalance` / `agora_getUtxos` |
| `agora_getConstitution` / `agora_getGovernance` | `agora_submitProposal` / `agora_castGovVote` / … |
| `agora_listProposals` / `agora_getProposal` / `agora_listOffices` | `agora_depositProposal` / tally / execute / forum post |
| `agora_listForumTopics` | `agora_ackConstitution` / sponsor / assent |

Clients (`agora-miner-sidecar`, stratum, faucet) forward `AGORA_RPC_TOKEN` as `Authorization: Bearer …`. Light clients accept optional `rpcToken` in `createLightClient`. Unauthorized calls return HTTP **401** with JSON-RPC error code `-32001`.

### Public bind

Non-loopback `AGORA_RPC_BIND` (e.g. `0.0.0.0:8545`) refuses to start unless `AGORA_RPC_ALLOW_PUBLIC_BIND=1`. Binding publicly without a token logs a warning.

`agora_getBlock` returns explorer-friendly JSON (`id`, hex parent hashes, `tx_count`, and hex `transactions` with inputs/outputs). Address fields in tx outputs / balance / UTXO responses are **Bech32m** (`agora1…`); request params still accept hex or Bech32m.  
`agora_getTransaction` returns `{ tx_id, status, block_id, index, fee, confirmations, transaction }` — wallets should poll until `confirmed` (missing txs return `status: "unknown"`, not an RPC error). Confirmed locations are indexed in `cf_warm` (`tx/` ‖ tx_id → block_id ‖ index) on admit / genesis. `confirmations` is blue-score depth vs the best tip (`max_tip_blue − block_blue + 1`) on live nodes (tip parent-distance on the in-memory test backend).  
`agora_getMempool` returns `{ count, transactions: [{ tx_id, fee, transaction }] }` ordered by fee desc then `tx_id` (default `limit` 128, max 10000).  
`agora_getNodeInfo` returns `{ network, version, peer_id, connected_peers, tip_count, mempool_count, pow_algorithm, bits, archival, hot_window, allow_fund, miner_address, genesis_hash }` (`network` is `dev`/`testnet`/…; miner as Bech32m; `genesis_hash` hex Block 0).  

`agora_getDrcPayment` accepts `{ "payment_id": "<64 hex>" }`, a one-element
array, or a bare hex ID. A canonical root-committed receipt returns
`{ payment_id, status: "settled", receipt }`; an absent receipt returns
`{ payment_id, status: "unknown", receipt: null }`. Receipt v1 exposes only
`result: "delivered_exact"` and equal base-unit `requested_amount` /
`delivered_amount`, plus fee, sender/recipient, source/destination tags, and
invoice ID. It excludes authorization bytes. Pending mempool state is
intentionally not coupled into this durable lookup, and `settled` does not by
itself assert checkpoint finality. Malformed IDs return `-32602`.

`agora_getDrcPaymentByInvoice` accepts either
`{ "recipient": "<account>", "invoice_id": "<64 hex>" }` or the positional
array `["<account>", "<64 hex>"]`. Account parsing follows existing RPC
conventions: a 20-byte hex account (optional `0x`) or an Agora Bech32m account;
the response normalizes it to Bech32m. Invoice IDs must decode to exactly 32
bytes. Malformed or missing values return `-32602`.

The response is:

```json
{
  "recipient": "agora1...",
  "invoice_id": "<64 lowercase hex>",
  "payment_id": "<64 lowercase hex or null>",
  "status": "settled",
  "receipt": {}
}
```

For a valid unknown tuple, invoice ID zero, wrong recipient, pending payment,
or rolled-back payment, `status` is `"unknown"` and both `payment_id` and
`receipt` are `null`. A settled response uses the same exact-delivery receipt
shape as `agora_getDrcPayment`, preserving source and destination tags while
excluding signatures and public keys. The method performs one
recipient-scoped index lookup and offers no invoice listing or prefix scan.
Here `settled` means accepted in the canonical state-machine virtual view, not
dual-PoS checkpoint finality.

`agora_getDrcAccountPolicy` accepts `{ "account": "<account>" }`, a one-element
array, or a bare 20-byte hex/Agora Bech32m account. A canonical DRC account
returns:

```json
{
  "account": "agora1...",
  "status": "known",
  "policy": {
    "version": 2,
    "require_destination_tag": true,
    "deposit_auth_required": true
  },
  "account_nonce": 4
}
```

An absent account returns `status: "unknown"` with null policy and nonce.
Malformed accounts return `-32602`. Missing policy state on a known account is
the canonical default-off policy.

`agora_submitDrcAccountPolicy` accepts a signed `DrcAccountPolicyTx`, optionally
wrapped as `{ "policy": ... }`, and returns `{ "policy_tx_id": "<hex>" }`.
The envelope contains only public authorization material; private keys are
never accepted. The owner signature binds chain, genesis, version, account,
set/clear action, shared nonce, and DRC fee. Pending submissions do not alter
the read query until state-machine acceptance. Neither a known policy nor an
accepted operation asserts PoW plus OVL/DRC checkpoint finality.

`agora_getDrcDepositPreauth` accepts either
`{ "owner": "<account>", "authorized_source": "<account>" }` or the positional
array `["<account>", "<account>"]`. If both canonical DRC accounts exist, the
response is:

```json
{
  "owner": "agora1...",
  "authorized_source": "agora1...",
  "status": "known",
  "preauthorized": true,
  "deposit_auth_required": true,
  "deposit_authorized": true
}
```

`preauthorized` reports the exact active record. `deposit_authorized` reports
the effective payment check: self, policy disabled, or exact record. If either
account is absent, `status` is `"unknown"` and all three booleans are null.
Malformed accounts, missing fields, and wrong positional arity return
`-32602`. The method is an exact point lookup: it cannot scan or enumerate
recipient grants and returns no signatures or keys.

`agora_submitDrcDepositPreauth` accepts a signed `DrcDepositPreauthTx`,
optionally wrapped as `{ "preauth": ... }`, and returns
`{ "preauth_tx_id": "<hex>" }`. It never accepts private-key material.
Pending grants/revokes do not alter the canonical read query. `"known"` and
accepted/settled results describe the canonical virtual view, not checkpoint
finality.

### Common DRC objects and accepted operations (Experimental)

`agora_getDrcObject` accepts `{ "object_id": "<64 hex>" }` and returns
`status: "live"` with a typed descriptor, or `status: "unknown"` with
`object: null`. `agora_getDrcAccountObjects` accepts:

```json
{
  "account": "agora1...",
  "kind": "escrow",
  "limit": 50,
  "cursor": null
}
```

`kind` and `cursor` are optional; `limit` defaults to 50 and must be
`1..=100`. The closed kind set is `account_policy`,
`deposit_preauthorization`, `regular_key`, `signer_list`, `ticket_set`,
`escrow`, `check`, `payment_channel`, `trust_line`, and
`issued_asset_policy`. Results are stably ordered by kind then object ID.
`next_cursor` is opaque and bound to the account and kind filter. Malformed,
altered, foreign-account, or filter-mismatched cursors return `-32602`.

`agora_getDrcOperation` accepts an `operation_id`; `agora_getDrcTransaction`
accepts a historical signed `transaction_id`. Each returns
`status: "accepted"` with the same typed accepted-operation receipt, or
`status: "unknown"` with `receipt: null`. Receipts include the canonical block
ID, application blue score, exact closed operation variant, semantic owner, and
sorted directly affected object IDs.

All four methods read only the canonical applied view. Reverting a block
atomically removes its orphan objects and receipts; pending mempool operations
are absent. `"live"` and `"accepted"` do not assert PoW plus both independent
PoS finality quorums. IDs and payloads are Agora SHA-256/Borsh shapes, not XRPL
ledger indexes, transaction hashes, wire encoding, or API parity. There is no
Offer/DEX or generic contract object in this surface.

### DRC payment channels (Experimental)

Submit methods accept optional lane wrappers (`payment_channel_create`, `payment_channel_fund`, `payment_channel_claim`, `payment_channel_close`) or bare transaction objects. Successful submit returns mempool acceptance (`channel_id`, `fund_tx_id`, `claim_tx_id`, or `close_tx_id` hex). **Accepted** means admitted to the local mempool and gossip — not PoW + dual-PoS finality.

`agora_getDrcPaymentChannel` returns `{ channel_id, status }` with `live` (open channel), `receipt` (closed receipt present), or `unknown`. Malformed `channel_id` → `-32602`.

`agora_verifyDrcPaymentChannelClaim` requires `channel_id`, `cumulative_authorized` (amount string/base units), and `channel_claim_signature` (hex). Verifies the off-ledger secp256k1 claim against the live channel `claim_public_key` and Agora domains. Unknown channel, bad signature, or bounds → `-32602`. Never accepts private keys.

Event getters (`agora_getDrcPaymentChannelFundEvent`, `ClaimEvent`, `ScheduleEvent`) are exact tx-id point lookups returning `known` or `unknown` without enumeration.

`agora_getBlockTemplate` returns `{ "block": Block, "randomx_epoch": u64 }` (native serde hashes as byte arrays). The block has a coinbase paying `AGORA_MINER_ADDRESS` for **emission + Σ transfer fees** at the estimated next blue score, followed by up to 128 mempool transfers (fee-desc, then `tx_id`); `header.tx_root` commits to that body. `randomx_epoch` is the blue-score–anchored RandomX key epoch miners must use. `agora_submitBlock` rejects `tx_root` mismatches and evicts included/conflicting mempool txs. Mempool admission requires `fee ≥ AGORA_MIN_RELAY_FEE`; fees are paid to the miner via the coinbase (not burned).

Example:

```bash
curl -s http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getDagTips","params":[]}'
```

The live backend (`NodeBackend`) reads tips/blocks/UTXOs from `StateStore`, admits signed transactions via `Mempool`, and publishes them on libp2p gossip.

## Light clients

`apps/shared/light-client` provides `createLightClient` + `startTipSync` / `watchTransaction` (optional `minConfirmations`) plus wallet helpers (`getBalance`, `getAccountBalances`, `getUtxos`, `submitTransaction`, BIP-39 `sendTransfer`) and query wrappers for TLT covenants and DRC DEX offers used by:

- `apps/explorer` (live DAG + tx lookup + mempool + node status + pending watch)
- `apps/desktop` (tip sync, UTXO lookup, signed send + confirmation poll)
- `apps/mobile` (tip sync, UTXO lookup, signed send + confirmation poll)

Default endpoint: `http://127.0.0.1:8545/rpc` (explorer/desktop may proxy `/rpc`).
