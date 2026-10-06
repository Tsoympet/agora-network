# Agora Network light client

**Maturity: Single-node prototype.**

The desktop (Tauri) and phone (Expo) wallets share one verifier in
`apps/shared/light-client`. It follows the whole Agora Network: TLT stays a
UTXO/RandomX asset, DRC stays a contract-free account asset, and OVL stays the
only programmable domain. The three balances are not merged.

This is not a Bitcoin SPV client and not an Ethereum light client. A phone or
PC still trusts one full node for GHOSTDAG parent selection, RandomX work, and
validator signatures.

## What a PC or phone user can do

1. Create or restore a BIP-39 wallet. The mnemonic is sealed with AES-256-GCM.
   Desktop stores the blob in `localStorage`. Phone stores it in Expo
   SecureStore. The node never receives the mnemonic or a private key.
2. Derive the `m/44'/8888'/0'/0/0` address once the node reports its network.
3. Read TLT (UTXO sum), OVL (account), and DRC (account) balances. The screen
   labels these as node-reported. They are not header proofs.
4. Submit a signed TLT transfer. Signing happens on the device.
5. Watch that transaction until the node reports pending or confirmed.
6. Verify a confirmed TLT transaction id: the wallet recomputes the header
   hash, the Merkle inclusion, and the body-root binding into `header.tx_root`.
7. Query one DRC ledger object by id. The result is the node's canonical
   object. The wallet refuses to call it header-proven.
8. See selected-parent header sync and a finality label computed from PoW flag
   plus independent OVL and DRC stake totals.
9. Open the **Capabilities** panel (desktop and phone) for the full feature
   matrix: TLT, DRC, OVL, and network reads with **verified locally**,
   **node-reported**, or **unavailable** labels. Spend wallets sign typed DRC/OVL
   envelopes on-device (`envelopes.ts`); watch-only wallets query only.

### Feature matrix (honest labels)

| Domain | Surface | Label |
| --- | --- | --- |
| Network | RPC URL, genesis binding, header spine, OVL/DRC quorum math | local + node |
| Network | Validator sets, native supply / fee burn | node when RPC exists |
| TLT | Receive, UTXO list, coin-select send, fee estimate, tx history | mixed |
| TLT | Merkle inclusion verify | local |
| TLT | Covenant / HTLC spends | **unavailable** on this branch |
| DRC | Policy, tags, DepositAuth, keys, multisign, tickets, escrow, checks, channels, trust lines, issued controls, owner objects | node |
| DRC | Typed sign + submit | local signing; watch-only fails closed |
| OVL | Native transfer, balance/nonce, validator views | mixed |
| OVL | EVM contract deploy UI | **unavailable** (no fake eth_call wallet) |

The canonical list lives in `apps/shared/light-client/featureMatrix.ts`. At
runtime the wallet probes JSON-RPC method names and marks rows unavailable when
the connected node does not implement them.

## What is verified on the device

| Check | Local? | Notes |
| --- | --- | --- |
| `BlockHeader` hash | Yes | Borsh + SHA-256, same as `BlockHeader::hash` |
| Selected-parent links | Yes | Each selected parent must be a listed parent, and blue score must rise toward the tip |
| Genesis binding | Yes | The spine must reach `agora_getNodeInfo.genesis_hash` |
| Reorg / header mismatch | Yes | The same header hash with different metadata is a mismatch. A new tip that drops the previous tip is a reorg |
| TLT Merkle inclusion | Yes | Pairwise SHA-256, same leaf rule as `Block::compute_tx_root` |
| TLT binding into `header.tx_root` | Yes | Body-lane wraps are folded locally. A missing layer fails |
| OVL execution id or DRC lane id in a block | Yes | Only when `agora_getBlockBinding` commits that id and the fold matches the header |
| OVL and DRC quorum math | Yes | Two-thirds of the supplied stake totals, checked separately |
| Wrong network | Yes | Normalized network id must match the node the wallet is using |

## What stays trusted

- RandomX (and any other PoW) is checked by the full node. `pow_checked_by` is
  `full_node`. The light client does not re-execute RandomX.
- GHOSTDAG blue sets are not recomputed. A dishonest node can offer a different
  spine that is internally linked. Comparing two nodes is future work.
- Validator signatures are not checked. Finality uses the stake counters on the
  checkpoint certificate. If `stake_fields_present` is false, the wallet does
  not treat the block as finalized.
- TLT, OVL, and DRC balances are full-node reads. `header_proven` is false.
  A payload that sets it to true is rejected.
- DRC ledger objects live in state, not in `BlockHeader.tx_root`. Asking for a
  header proof fails closed. There is no contract or EVM proof on DRC.
- OVL account balances and execution results are not proven against a state
  root inside the live header. An OVL execution id can be shown to sit in a
  block body when the binding lists it. That is inclusion of the id, not EVM
  receipt verification. This client does not add Ethereum JSON-RPC.

## RPC the wallets call

Public reads (no bearer token): `agora_getLightHeaders`,
`agora_getBlockBinding`, `agora_getTltInclusionProof`, plus the existing block
and DRC object reads.

Token-gated, same as balance and submit: `agora_getNativeBalances`,
`agora_getBalance`, `agora_getUtxos`, `agora_submitTransaction`.

Header windows are capped at 512. A window that stops before genesis makes
verification fail closed.

## PC

```bash
# full node
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# wallet UI (browser or Tauri)
cd apps/desktop
npm install
npm run dev
```

`VITE_AGORA_RPC_URL` defaults to `http://127.0.0.1:8545/rpc`. Set
`VITE_AGORA_RPC_TOKEN` when the node requires `AGORA_RPC_TOKEN`.

## Phone

```bash
cd apps/mobile
npm install
npm start
```

`EXPO_PUBLIC_AGORA_RPC_URL` must be a LAN address on a physical device.
`EXPO_PUBLIC_AGORA_RPC_TOKEN` is the optional bearer token. The vault adapter
calls SecureStore's async methods (`getItemAsync`, `setItemAsync`,
`deleteItemAsync`).

## Checking the same wallet away from home

Pairing is device-to-device. The PC and the phone do not share a cloud Agora
account, an iCloud keychain, or a bridge. After pairing, each device talks to
a full node on its own. The PC can be off.

Both wallets show the same short guide: use the same network, compare genesis
before import, let the phone recompute the selected-parent spine to that
genesis, and set a reachable RPC when you leave the house.

### Watch-only (recommended for travel)

1. On the PC, unlock or create the wallet and wait until the node reports a
   genesis hash.
2. If the PC RPC is `127.0.0.1`, open **Pair with another device**, set an
   `http` or `https` URL the phone can reach, and save it. Localhost is left
   out of the pairing code.
3. Show the watch-only code, or copy the payload. It contains the TLT receive
   address, the TLT change address, the OVL account id, the DRC account id,
   the account xpub at `m/44'/8888'/0'`, the network, and the genesis. It does
   not contain a mnemonic, an xprv, or the node token.
4. On the phone, scan the code or paste the payload. Import stays disabled
   until the phone's node reports the same network and the same genesis.
5. Look up balances. TLT, OVL, and DRC figures are node-reported. Confirmed TLT
   inclusion is still checked on the phone. **Sign & send** fails closed:
   `watch-only wallet cannot sign or spend`.

### Same-spend restore

1. On the PC, unlock the spend wallet.
2. Acknowledge that anyone with the mnemonic can spend, then show the
   reveal-once code or copy the payload. Hide it as soon as the phone has it.
   The words are not written to logs.
3. On the phone, scan, paste, or type the same BIP-39 words into the mnemonic
   field. Import of the payload waits for a matching genesis.
4. Save a password vault on the phone if it should keep the words. After that,
   the phone derives the same `m/44'/8888'/0'/0/0` address and can sign without
   the PC being online.

The restore payload is versioned JSON (`agora-light-restore-v1`) with the
mnemonic, network, and genesis only. The BIP-39 passphrase is empty, matching
the current vault. A non-empty BIP-39 passphrase is not part of this prototype.

### RPC away from a home node

The desktop and phone both persist a user-chosen RPC URL
(`agora.light.rpc.v1`) and an optional node bearer token
(`agora.light.rpc.token.v1`). The token never enters a pairing payload.

- `http` and `https` are accepted. Anything else, and URLs with embedded
  usernames or passwords, are rejected.
- Plain HTTP warns that path attackers can alter what the device is shown.
- Localhost warns that another device cannot use that endpoint.
- A LAN address works on the same network and does not follow you off that
  network.
- Node-reported balances trust that endpoint. Proof-checked TLT inclusion is
  still recomputed on the device.

## Threat model

An attacker who serves the wallet's RPC can:

- hide transactions or balances
- present a self-consistent selected-parent history that is not the network's
- report stake totals that do not correspond to real validator signatures

An attacker who only modifies a Merkle sibling, drops a body-binding layer,
mismatches the genesis hash, or omits quorum fields is rejected on the device.

Private keys are not an RPC feature. Submit paths take an already signed
transaction.

## Community

Passport, missions, academy, grants, and the other community modules live in
`apps/shared/community` and are documented in
[`agora-community.md`](agora-community.md). They are a light-client surface:
headers and wallet signatures stay on the device, and community records are
not treated as consensus. The PC nav and the phone nav both open those
screens. A community session token is not a seed, and a watch-only wallet
still cannot sign a spend.

Pairing adds these cases:

- A watch-only payload cannot sign. An xprv, a mnemonic field, a localhost
  RPC, or an address that does not match the xpub is rejected.
- The account xpub lets the holder derive later receive and change addresses.
  That is visibility, not spend authority.
- A restore code or a photo of it is the spend key. Hiding the screen does not
  help after someone has copied it. Prefer watch-only for travel.
- A node that matches the expected genesis can still lie about balances and
  about which spine is the network's. The genesis check stops a random chain.
  It does not stop a node that replays the real genesis and then forks.

## Security model

Headers, Merkle proofs, state proofs, RPC, indexers, the community API, the
wallet, passport export, governance badges, and confirmation are listed in
[`LIGHT_CLIENT_SECURITY_MODEL.md`](LIGHT_CLIENT_SECURITY_MODEL.md). The desktop
and phone trust panels render that full list, including every assumption.
