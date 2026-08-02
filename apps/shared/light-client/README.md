# Agora light-client helpers

Shared HTTP JSON-RPC tip sync for desktop, mobile, and explorer.

```ts
import { createLightClient, startTipSync } from "../shared/light-client";

const client = createLightClient({ rpcUrl: "http://127.0.0.1:8545/rpc" });
const stop = startTipSync({
  client,
  pollMs: 2000,
  onUpdate: (snap) => console.log(snap.status, snap.tips),
});
// later: stop();
```

Methods used: `agora_getDagTips`, `agora_getBlock`.
