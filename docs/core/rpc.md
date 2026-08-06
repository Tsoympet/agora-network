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
| `agora_getBalance` | Address balance (sum of live `cf_utxo`) |
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
| `agora_submitStakeTx` | Admit secp256k1-signed stake tx (bond/delegate/unbond/withdraw) |
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

Civic state is persisted under RocksDB Meta key `meta/governance` (`CivicSnapshot`).

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
| `agora_getDagTips` / `agora_getBlock` / `agora_getTransaction` | `agora_getBlockTemplate` / `agora_fundAddress` |
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

`agora_getBlockTemplate` returns `{ "block": Block, "randomx_epoch": u64 }` (native serde hashes as byte arrays). The block has a coinbase paying `AGORA_MINER_ADDRESS` for **emission + Σ transfer fees** at the estimated next blue score, followed by up to 128 mempool transfers (fee-desc, then `tx_id`); `header.tx_root` commits to that body. `randomx_epoch` is the blue-score–anchored RandomX key epoch miners must use. `agora_submitBlock` rejects `tx_root` mismatches and evicts included/conflicting mempool txs. Mempool admission requires `fee ≥ AGORA_MIN_RELAY_FEE`; fees are paid to the miner via the coinbase (not burned).

Example:

```bash
curl -s http://127.0.0.1:8545/rpc \
  -H 'content-type: application/json' \
  -d '{"id":1,"method":"agora_getDagTips","params":[]}'
```

The live backend (`NodeBackend`) reads tips/blocks/UTXOs from `StateStore`, admits signed transactions via `Mempool`, and publishes them on libp2p gossip.

## Light clients

`apps/shared/light-client` provides `createLightClient` + `startTipSync` / `watchTransaction` (optional `minConfirmations`) plus wallet helpers (`getBalance`, `getUtxos`, `submitTransaction`, BIP-39 `sendTransfer`) used by:

- `apps/explorer` (live DAG + tx lookup + mempool + node status + pending watch)
- `apps/desktop` (tip sync, UTXO lookup, signed send + confirmation poll)
- `apps/mobile` (tip sync, UTXO lookup, signed send + confirmation poll)

Default endpoint: `http://127.0.0.1:8545/rpc` (explorer/desktop may proxy `/rpc`).
