# Agora Desktop

Vite wallet shell (Obsidian & Gold) with HTTP JSON-RPC tip sync. Tauri packaging
hooks live under `src-tauri/`; day-to-day development uses `npm run dev`.

## Features

- Bech32m receive (`agora1…`) with hex secondary + copy
- BIP-39 generate / derive (`m/44'/8888'/0'/0/0`) and signed send
- Compact node strip via `agora_getNodeInfo` (mempool, PoW, peers, archival)
- Post-send pending → confirmed + confirmation depth

```bash
# Terminal A
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# Terminal B
cd apps/desktop
npm install
npm run dev
```

| Env | Default | Meaning |
| --- | --- | --- |
| `VITE_AGORA_RPC_URL` | `http://127.0.0.1:8545/rpc` | Node JSON-RPC |
| `VITE_AGORA_POLL_MS` | `2000` | Tip poll interval |

Shared client: `apps/shared/light-client`.
