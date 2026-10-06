# Agora data-plane split

**Maturity: Scaffold.**

Agora keeps three planes. A PC or phone light client is the device plane. Operator
services are the infrastructure plane. Consensus records are the on-chain plane.
The shared labels live in `apps/shared/data-plane/features.ts`. Wallet capability
rows in `apps/shared/light-client/featureMatrix.ts` carry the same `plane` field.

Full-node JSON-RPC for canonical state is the on-chain read path. It is not the
indexer. The indexer is an operator cache and never a light-client proof.

## Planes

| Plane | What it holds | What it must not hold |
| --- | --- | --- |
| Device | Wallet, light verification, Passport UI, community UI, DRC payment UI, QR, missions UI, Academy UI, governance UI, merchant tools, notifications UI, explorer UI, guilds UI | Forum backend, indexer, grant and mission admin, event service, search engine, moderation pipeline, notification dispatcher |
| Infrastructure | Indexing, forum backend, grant and mission management, event services, notification dispatch, search, moderation, and other non-decentralized services | A claim that those records are consensus |
| On-chain | DRC, OVL, and TLT balances and transactions, governance transactions where a full node committed them, treasury transactions, and verifiable credentials or attestations where one is justified | Private profiles, forum posts, mission reviews, and Academy progress |

Device code calls infrastructure with `fetch` through `createDeviceInfrastructureClient`.
The PC, phone, and explorer do not import `infrastructure/indexer` or
`infrastructure/forum-server`. An empty infrastructure URL fails closed. It does
not boot an in-process server and it does not invent a balance.

## Feature descriptors

| Id | Plane | Device | Infrastructure | On-chain |
| --- | --- | --- | --- | --- |
| `wallet` | device | Vault and signing | — | — |
| `light-verification` | device | Header, Merkle, and quorum checks | — | — |
| `passport-ui` | device | Passport screen | Optional profile store | Attestation only when one is signed |
| `community-ui` | device | Renders HTTP results | Directory and forum | — |
| `drc-payments` | device | QR, amount, local sign | — | A real `DrcPaymentTx` only |
| `qr` | device | Parse and encode | — | — |
| `missions-ui` | device | List and submit | Grant and mission admin | — |
| `academy-ui` | device | Course screen | Catalog host | — |
| `governance-ui` | device | Areas and polls | Advisory polls | Committed governance transactions |
| `merchant-tools` | device | Invoices, no spend keys | Merchant directory | — |
| `notifications-ui` | device | Preferences, amount-free copy | Dispatcher | — |
| `explorer-ui` | device | Renders node rows | Optional indexer, not bundled | Canonical blocks and transactions |
| `guilds-ui` | device | Charter screen | Membership records | — |
| `indexer` | infrastructure | Status over HTTP | `infrastructure/indexer` | — |
| `forum-backend` | infrastructure | HTTP only | `infrastructure/forum-server` | — |
| `grant-mission-admin` | infrastructure | HTTP only | `infrastructure/grant-admin` | — |
| `event-service` | infrastructure | HTTP only | `infrastructure/event-service` | — |
| `notification-dispatch` | infrastructure | Token registration only | `infrastructure/notification-dispatch` | — |
| `search` | infrastructure | HTTP only | `infrastructure/search` | — |
| `moderation` | infrastructure | HTTP only | `infrastructure/moderation` | — |
| `tlt-balances-txs` | on-chain | Node read plus local TLT inclusion | — | UTXOs and spends |
| `ovl-balances-txs` | on-chain | Sign and read | — | Account balances and execution |
| `drc-balances-txs` | on-chain | Typed envelopes | — | Account balances and payments |
| `governance-txs` | on-chain | Show a node commitment | Advisory polls stay off chain | Committed governance transactions |
| `treasury-txs` | on-chain | `agora_getProtocolTreasuries` only | — | Treasury balances and transactions |
| `credential-attestations` | on-chain | Display a committed attestation | — | Justified credentials only |
| `private-profile` | device | Email and language | Optional authenticated store | Forbidden |
| `forum-posts` | infrastructure | Rendered in the community UI | Forum server | Forbidden |
| `mission-reviews` | infrastructure | Not stored on the device | Grant and mission admin | Forbidden |
| `academy-progress` | device | Local lesson ids | — | Forbidden |

`private-profile`, `forum-posts`, `mission-reviews`, and `academy-progress` are
not consensus. An Academy certificate or a passport attestation is a different
record, `credential-attestations`, and only when that attestation exists.

## Trust

Infrastructure is an operator. The operator can omit, reorder, delay, or alter
what the service returns. Every infrastructure HTTP body is an envelope:

- `plane` is `infrastructure`
- `chainProof` is `false`
- `chainData` is `null`
- `trust` says who can lie
- `data` is the service payload

The facade rejects a body that claims a chain proof or attaches chain data beside
the payload. The indexer does not invent balances, UTXOs, or transactions. With
no `AGORA_INDEXER_UPSTREAM_RPC` it returns no chain rows. A proxied full-node
response stays `chainProof: false` because the device did not recompute it.

| Service | Trust |
| --- | --- |
| `indexer` | Not a second consensus and not a header proof. Empty upstream means no chain rows. |
| `forum-backend` | The operator can omit, reorder, or alter posts. Posts are not votes and not treasury spends. |
| `grant-mission-admin` | Transitions and reviews do not move treasury funds. |
| `event-service` | Schedules are operator records. |
| `notification-dispatch` | The device does not run the send loop. Payloads must not include amounts or balances. |
| `search` | Hits are community documents, not chain state. A hub name filter on rows the device already holds is UI, not this engine. |
| `moderation` | A report does not change reputation or consensus. |

The administrative civic forum RPC on a full node (`agora_postForumTopic`) is
local node state. It is not this forum server and it is not compiled into the
PC, phone, or explorer.

## Clients

`createCommunityClient` and the explorer's `infrastructureClient` use the
facades. Mission advances and forum reports are HTTP calls. When the
infrastructure origin is unset, those calls report that the device did not
record them.

Treasury screens show `agora_getProtocolTreasuries` or nothing. The community
host has no `/treasury` route. A missing node read does not invent a balance.

Set `VITE_AGORA_COMMUNITY_URL` or `EXPO_PUBLIC_AGORA_COMMUNITY_URL` to the
community host. The explorer uses `VITE_AGORA_INFRA_URL` for the same kind of
origin. Chain RPC stays on the existing node URL.
