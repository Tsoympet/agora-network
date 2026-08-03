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

RPC methods: `agora_getDagTips`, `agora_getBlock`, `agora_getTransaction`,
`agora_getMempool`, `agora_getNodeInfo`, `agora_getBalance`, `agora_getUtxos`,
`agora_submitTransaction`.

Addresses: Bech32m (`agora1…`) preferred; 40-char hex still accepted via
`parseAddress`.
