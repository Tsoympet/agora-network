# Canonical DRC ledger-object identity and query index

**Maturity:** Experimental

This module gives existing protocol-native DRC state a common identity,
owner-directory, and accepted-operation query surface. It is an Agora
adaptation of XRPL ledger objects and `account_objects`; it is not XRPL binary,
hash, address, reserve, amendment, or API parity.

It does not add a transaction family or reinterpret historical block or
transaction bytes. Object descriptors mirror canonical typed state, and all
index mutations are committed in the same batch and rollback journal as the
state transition that created, updated, or deleted the source object.

## Identity

An object ID is:

```text
SHA-256(Borsh(
  "agora-trident-drc-ledger-object-id-v1",
  DrcLedgerObjectKey
))
```

`DrcLedgerObjectKey` is a closed enum. Its kind and primary key are part of the
domain-separated preimage, so different object families cannot alias merely
because they contain the same account or hash bytes. Locked vectors cover
every supported kind. These IDs are Agora identifiers and do not reproduce
XRPL ledger indexes.

`DrcLedgerObjectDescriptor` contains the version, object ID, kind, semantic
owner, typed key, and typed live object. Reads validate all redundant identity
fields against both the deterministic ID and the canonical source record.

## Exact live-object coverage

| Kind | Deterministic key | Owner-directory account | Canonical source |
| --- | --- | --- | --- |
| `account_policy` | account | account | DRC account-policy record |
| `deposit_preauthorization` | owner + authorized source | owner | live address preauthorization |
| `regular_key` | owner | owner | installed regular key |
| `signer_list` | owner | owner | installed weighted signer list |
| `ticket_set` | owner | owner | persisted sorted outstanding-ticket set |
| `escrow` | escrow ID | escrow owner | live native-DRC escrow |
| `check` | Check ID | Check owner | live native-DRC Check |
| `payment_channel` | channel ID | channel owner | live native-DRC payment channel |
| `trust_line` | holder + `IssuedAssetId` | holder | live issuer-liability trust line |
| `issued_asset_policy` | `IssuedAssetId` | issuer | live issued-asset control policy |

The owner index is sparse and ordered by `(kind, object_id)`. Queries seek
directly to `owner || optional-kind || cursor`, return at most 100 entries, and
never enumerate the full Meta column family. Cursors are opaque, versioned,
bound to the owner and optional kind filter, and carry a domain-separated
integrity hash. Malformed, altered, foreign-owner, and filter-mismatched
cursors fail as invalid parameters.

## Accepted-operation receipts

Every `TransactionAcceptance::Accepted` DRC operation gets a
`DrcAcceptedOperationReceipt`. The closed operation union covers native DRC
account transfers and stake operations plus payments, policy and key changes,
Tickets, escrow, Checks, payment channels, trust lines, issued transfers, and
issued-asset controls. `ExactDuplicate`, `ConflictLost`, and rejected
operations get no receipt.

The operation ID is domain-separated from the historical signed transaction
ID and includes the operation kind. A second index maps the historical
transaction ID to exactly one operation receipt; ambiguity or replay fails
closed. Each receipt records:

- the canonical block ID and application blue score;
- the complete typed accepted operation;
- its semantic owner and operation kind; and
- the sorted IDs of directly affected common ledger objects.

For detached multisign blocks, the canonical block ID continues to hash the
frozen detached block bytes. The receipt operation is reconstructed with its
body-root-committed multisign authorization, so transaction lookup uses the
same signed envelope ID returned before template materialization. Orphan,
duplicate, wrong-kind, or otherwise ambiguous attachment rows are rejected
before state mutation.

## Storage, root, rollback, and restart

Schema 21 stores the marker, descriptors, owner rows, receipts, and
transaction mappings under `ledger/drc/` in Meta. The
`agora-trident-drc-ledger-object-index-root-v1` component commits sorted
descriptors and accepted receipts into composed state-root domain
`agora-trident-state-root-v15`.

Before a block commits, every touched common-index key is snapshotted once in
`UtxoJournal.drc_ledger_index_meta_before`. Revert therefore removes orphan
objects and receipts and restores changed/deleted objects atomically with
balances, locked value, Tickets, fee burn, and family state. Reapplying the
same canonical operation recreates the same IDs and root.

Startup at schema 21 verifies that:

- the marker and versions are exact;
- descriptors exactly mirror every supported live source record;
- owner rows exactly match descriptors; and
- receipt and transaction indexes are internally complete and one-to-one.

Explicit reindex rebuilds only derivable live-object and owner rows. It
preserves receipts, validates the complete result on a copy-on-write overlay,
and commits nothing if receipt history is inconsistent.

The schema 20→21 migration requires canonical applied-blue order plus retained
block bodies, acceptance records, and rollback journals. It reconstructs
accepted receipts, rebuilds live descriptors, rewrites each historical
journal for later reorg, validates the result on an overlay, and commits once.
Missing history, duplicate order entries, partial pre-existing index data, or
detached authorization without the correct chain/genesis context aborts the
migration. A pruned legacy schema-20 node missing required bodies must resync
from retained history; migration never guesses.

## Public query surface

- `agora_getDrcObject`
- `agora_getDrcAccountObjects`
- `agora_getDrcOperation`
- `agora_getDrcTransaction`

The shared light client exposes matching typed methods. OpenAPI lists the
closed object-kind set and the `1..=100` page bound. These methods return only
canonical accepted state; pending mempool operations and red/orphaned branch
receipts are intentionally absent.

## Explicit exclusions

The common directory does not expose native balance/sequence state as an
XRPL-style `AccountRoot`, issuer-liability aggregates, family-specific settled
event receipts, governance, staking/finality records, or pending operations as
live account objects. Those states retain their existing typed point queries.

There is no `Offer` kind, Offer transaction, order-book index, path payment,
autobridging, rippling, AMM, NFT, Hook, EVM, bytecode, deploy/call, or generic
contract object. Native DRC remains distinct from `IssuedAssetId`, and no
issuer control can target native DRC. The subsequent Offer engine must add a
reviewed typed object and transition; this foundation does not claim DEX
support or define owner-reserve economics.

## Protocol profile

| Surface | Value |
| --- | --- |
| Trident protocol | `23` |
| Transaction signing profile | `agora-trident-tx-v9` |
| State transition | `agora-trident-state-v21` |
| State-root domain | `agora-trident-state-root-v15` |
| Datadir schema | `21` |
| Common-index schema | `1` |
| Genesis | v3 draft, `UNFROZEN`; no live Trident loader |

The Trident fingerprint commits these protocol and state-transition values.
Frozen pre-Trident network constants are unchanged.
