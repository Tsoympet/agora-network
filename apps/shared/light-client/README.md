# Agora light-client helpers

Shared HTTP JSON-RPC tip sync and wallet queries for desktop, mobile, and explorer.

```ts
import { createLightClient, startTipSync } from "../shared/light-client";

const client = createLightClient({ rpcUrl: "http://127.0.0.1:8545/rpc" });
const stop = startTipSync({
  client,
  pollMs: 2000,
  onUpdate: (snap) => console.log(snap.status, snap.tips),
});

const { balance } = await client.getBalance("<40-hex address>");
const { utxos } = await client.getUtxos("<40-hex address>");
// later: await client.submitTransaction(signedTxJson);
// later: stop();
```

Methods: `agora_getDagTips`, `agora_getBlock`, `agora_getBalance`, `agora_getUtxos`, `agora_submitTransaction`.
