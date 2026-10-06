# Agora light-client helpers

Shared HTTP JSON-RPC tip sync and wallet helpers for desktop, mobile, and explorer.

```ts
import {
  addressBech32FromMnemonic,
  createLightClient,
  generateMnemonic,
  parseAddress,
  startTipSync,
} from "../shared/light-client";

const client = createLightClient({ rpcUrl: "http://127.0.0.1:8545/rpc" });
const stop = startTipSync({
  client,
  pollMs: 2000,
  onUpdate: (snap) => console.log(snap.status, snap.tips),
});

const mnemonic = generateMnemonic(128);
const bech32 = addressBech32FromMnemonic(mnemonic);
const hex = parseAddress(bech32);

const { balance } = await client.getBalance(bech32);
const info = await client.getNodeInfo();
// later: stop();
```

Network light checks (`agoraLight.ts`) recompute header hashes, selected-parent
links, TLT Merkle proofs, and body-root bindings. They fail closed when a
proof or quorum total is missing. Device pairing (`pairing.ts`) is either a
public watch-only payload or a reveal-once BIP-39 restore. See
[`docs/core/agora-light-client.md`](../../../docs/core/agora-light-client.md).

RPC methods: `agora_getDagTips`, `agora_getBlock`, `agora_getLightHeaders`,
`agora_getBlockBinding`, `agora_getTltInclusionProof`, `agora_getNativeBalances`,
`agora_getTransaction`,
`agora_getMempool`, `agora_getNodeInfo`, `agora_getBalance`, `agora_getUtxos`,
`agora_submitTransaction`.

Addresses: Bech32m HRPs `agora` / `agoratest` / `agoradev` preferred; 40-char
hex stays network-neutral. Pass the node network to `parseAddress(input, network)`
(and to `sendTransfer` / `buildSignedTransfer`) so cross-network Bech32 recipients
are rejected instead of silently re-encoded.
