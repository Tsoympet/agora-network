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

## Threat model

An attacker who serves the wallet's RPC can:

- hide transactions or balances
- present a self-consistent selected-parent history that is not the network's
- report stake totals that do not correspond to real validator signatures

An attacker who only modifies a Merkle sibling, drops a body-binding layer,
mismatches the genesis hash, or omits quorum fields is rejected on the device.

Private keys are not an RPC feature. Submit paths take an already signed
transaction.
