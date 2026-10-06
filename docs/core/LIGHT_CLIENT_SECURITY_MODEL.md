# Light client security model

**Maturity: Single-node prototype** for header and TLT inclusion checks.
**Maturity: Scaffold** for hardware wallets, hardware biometrics, and native screenshot blocking.

This note sits beside [`agora-light-client.md`](agora-light-client.md) and the
architecture sections of [`agora-community.md`](agora-community.md). The PC and
phone apps stay light clients. They do not run RandomX, recompute GHOSTDAG, or
check validator signatures.

## Keys stay on the device

- The mnemonic is sealed with AES-256-GCM and PBKDF2-SHA-256 (210,000
  iterations) before it is stored. Desktop persistence is `localStorage`.
  Phone persistence is Expo SecureStore (`agora.wallet.vault.v1`) through the
  same blob format.
- A community session token is not a seed. `exportCommunitySession` rejects
  mnemonic, seed, xprv, and password fields.
- A spend session keeps the phrase in a side table, not on the session object.
  Lock and timeout delete it. `exportDeviceSeed` throws `locked session cannot
  export seed` unless the session is live, and it still requires an explicit
  reveal. Role checks never grant seed export.
- Watch-only pairing payloads do not carry a mnemonic or xprv. Watch-only
  mode cannot sign.
- RPC bearer tokens stay in the vault storage adapter. They are not pairing
  fields and they are not community records.

## What the device verifies

| Check | Label |
| --- | --- |
| Selected-parent header hash and ancestry | verified locally |
| TLT Merkle inclusion and body binding | verified locally |
| OVL and DRC quorum math on supplied stake totals | verified locally, not signature checks |
| DRC ledger objects | node-reported. A header proof fails closed |
| Balances | node-reported. `header_proven: true` is rejected |
| OVL contract call / EVM receipt | **PLANNED**. Nonempty calldata is rejected |

## What a node or community service can lie about

An RPC endpoint can hide transactions, offer a self-consistent spine that is
not the network, and report stake totals that do not match validator
signatures. An indexer can be stale about ordering, completeness, and derived
balances. A community API can be wrong about display names, drafts, and
fixture bundles. None of those sources may be treated as spend authorization
or finality.

Service-by-service boundaries live in `apps/shared/core/services.ts`.

## PC

- Vault ciphertext version stays 1 so existing blobs still open. A stronger
  KDF is **PLANNED** rather than a silent rewrite.
- The wallet locks on the session timer (default five minutes of inactivity
  on desktop). Lock clears the mnemonic from memory.
- Node-address warnings cover plaintext HTTP, localhost, embedded credentials,
  punycode or non-ASCII hosts, and a host that does not match the saved
  endpoint.
- Transaction preview shows destination, amount, and fee with `signed: false`
  and `submitted: false`. A signer runs only after the preview id is confirmed.
- USB hardware signers implement `HardwareSigner` and return **PLANNED**. No
  USB signer is linked.

## Phone

- SecureStore holds the sealed vault, not a raw seed.
- PIN unlock stores a salted hash and `holdsSeed: false`. Hardware biometrics
  and secure-element keys are **PLANNED**.
- The mnemonic field stays hidden until the user chooses Show. Generate and
  unlock do not reveal it.
- Clipboard copy of a mnemonic or restore payload requires an explicit reveal
  and should be cleared (30 seconds). Address copy is allowed.
- Screenshot blocking is **PLANNED** on iOS and Android. Web can only mark the
  document, which does not stop a screenshot.

## Implementation rule

This client does not fabricate confirmations. Community reads from cache set
`confirmed: false`. The community DRC pay path reports broadcast `unavailable`
and `confirmed: false` instead of submitting a look-alike transaction. In-process
service adapters return empty **PLANNED** lists.
