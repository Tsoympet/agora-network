# Light client security model

**Maturity: Single-node prototype.**

Desktop (Tauri) and phone (Expo) wallets share the verifier in
`apps/shared/light-client`. Community trust screens in
`apps/shared/community-trust` stay beside that verifier. They do not add a
second proof system, and they do not move keys off the device.

This page lists every assumption the in-app trust panel shows. The panel
renders the full list. It has no collapse control and no hidden row.

A PC or phone still trusts one full node for GHOSTDAG parent selection,
RandomX work, and validator signatures. Comparing two nodes is future work.

## Assumption index

`headers`, `merkle`, `state_proofs`, `rpc`, `indexer`, `community_api`, `wallet`, `passport`, `governance`, `confirmation`

## Headers

**Posture:** verified locally.

**What the device checks:** The device recomputes header hashes, checks selected-parent links, and binds the spine to the genesis hash from the node the wallet is using.

**Assumption:** RandomX work and the GHOSTDAG blue set are computed by that full node. A node can serve a self-consistent spine that is not the public network. This client does not compare a second node.

Header windows are capped at 512. A window that stops before genesis fails closed. The same header hash with different metadata is a mismatch. A new tip that drops the previous tip is a reorg.

## Merkle

**Posture:** verified locally.

**What the device checks:** For a TLT transaction id, the device recomputes the pairwise SHA-256 Merkle path and the body-root fold into header.tx_root.

**Assumption:** The full node chooses which siblings to serve. A bad sibling fails the local check. The proof shows inclusion of that transaction id in that header. It does not prove an account balance or the absence of other transactions.

OVL execution ids and DRC lane ids are checked the same way only when `agora_getBlockBinding` lists them and the fold matches the header. That is inclusion of an id.

## State proofs

**Posture:** not proven.

**What the device checks:** The device rejects a balance or DRC object payload that claims header_proven.

**Assumption:** TLT, OVL, and DRC balances and DRC ledger objects are full-node state reads. The live header this client verifies has no state-root proof for those balances. OVL execution inclusion, when a binding is present, shows an id in a block body and does not verify an EVM receipt.

DRC has no contract or EVM proof. Asking for a header proof of a DRC object fails closed.

## RPC

**Posture:** node-reported.

**What the device checks:** The wallet stores the RPC URL the user entered and checks the reported network id and genesis hash against that pairing.

**Assumption:** One RPC endpoint can hide transactions, balances, and votes. Bearer tokens stay in device storage. The node does not receive a mnemonic or a private key on submit. Submit sends an already signed transaction.

Public reads used for proofs are `agora_getLightHeaders`, `agora_getBlockBinding`, and `agora_getTltInclusionProof`. Balances and submit stay token-gated when the node is configured that way. Plain HTTP is visible to the path. Embedded usernames and passwords in the URL are rejected.

## Indexer

**Posture:** operator-reported.

**What the device checks:** This client does not treat an indexer as a proof source.

**Assumption:** An indexer, if one is added later, is an operator database. It can be incomplete or wrong. Confirmation and inclusion still require the header and Merkle checks. This build does not query an indexer for trust.

## Community API

**Posture:** operator-reported.

**What the device checks:** Local community search runs on device records. Seed examples are data, not a country enum, and the search box has no GPS input.

**Assumption:** A remote community API is not consensus. Hub, grant, mission, and analytics responses would be operator-reported. This build does not send identity exports or keys to a community API.

Seed records for Greece, Europe, Asia, and the Americas ship as data. Search is the text the user types. The client has no GPS call.

## Wallet

**Posture:** device-local.

**What the device checks:** BIP-39 generation, AES-256-GCM vault sealing, and secp256k1 signing happen on the device. Desktop uses localStorage. Phone uses SecureStore.

**Assumption:** The vault password and mnemonic are not RPC parameters. Watch-only sessions have an account key and no spend key. A compromised RPC cannot sign by itself. A compromised device can.

The account path is `m/44'/8888'/0'/0/0`. Pairing can export a watch-only xpub or, if the user explicitly reveals it, a restore mnemonic. The restore payload is the spend key. Watch-only cannot sign.

## Passport

**Posture:** device-local.

**What the device checks:** The portable export is JSON built on the device. The builder refuses mnemonic, seed, and private-key fields. Service trust labels are attached to the bundle.

**Assumption:** Exporting a passport does not prove the issuer signature. This build does not re-verify hub signatures during export. Attestations are contribution evidence. They are not personhood and they are not Sybil resistance. Local preference fields stay on the device until the user copies the file.

The open types are `agora-portable-identity-v1` in
[`docs/community/portable-identity.openapi.yaml`](../community/portable-identity.openapi.yaml).
`contains_private_keys` is false. A 12-word region query is refused so a
pasted mnemonic does not land in the preference object.

## Governance

**Posture:** mixed.

**What the device checks:** Each proposal view carries a derived ON-CHAIN or ADVISORY badge. Advisory records cannot be given a binding passed result. Tallies shown here are counts.

**Assumption:** An advisory proposal does not change consensus. An on-chain tally read from one node is node-reported. This client does not re-check ballot signatures or chamber eligibility. Shipped status requires a commit or release link, and that link is not itself a consensus proof.

The badge is computed from `authority`. A stored badge cannot disagree with that authority, because the record does not keep a second badge field.

## Confirmation

**Posture:** mixed.

**What the device checks:** The wallet can watch a transaction until the node reports pending or confirmed, and it can verify a TLT Merkle proof locally when the node serves one.

**Assumption:** A single node's confirmed status is not network finality. Finality on this client is the PoW flag plus independent two-thirds OVL stake and two-thirds DRC stake, using the stake totals on the checkpoint. Those totals are not validator signature checks. If stake fields are absent, the block is not shown as finalized.

OVL and DRC quorum math is local arithmetic on the totals the node supplied. The signatures behind those totals are not checked here.

## What this model leaves in place

- Keys stay on the device that created or restored them.
- TLT monetary policy is unchanged. Community screens cannot mint TLT to fund a reward.
- Public analytics on the community screen are aggregate counters. The parser rejects per-user fields.
- The light client remains a verifier of headers and Merkle paths plus honest labels for everything else.
