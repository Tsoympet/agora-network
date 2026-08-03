# Agora Desktop

Vite wallet shell (Obsidian & Gold) with HTTP JSON-RPC tip sync. Tauri packaging
hooks live under `src-tauri/`; day-to-day development uses `npm run dev`.

## Features

- Clear **Devnet / Testnet / Mainnet** badge (from `agora_getNodeInfo`)
- Bech32m receive with network HRP (`agoradev` / `agoratest` / `agora`) + hex secondary
- BIP-39 generate / derive (`m/44'/8888'/0'/0/0`) and signed send
- Password vault (AES-256-GCM) persisted in `localStorage` — Unlock / Save / Lock
- Compact node strip via `agora_getNodeInfo` (mempool, PoW, peers, archival)
- Post-send pending → confirmed + confirmation depth

```bash
# Terminal A
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# Terminal B
cd apps/desktop
npm install
npm run dev
# native installers (host OS): npm run tauri:build
```

Multi-OS targets: [`docs/apps/PLATFORMS.md`](../../docs/apps/PLATFORMS.md).

| Env | Default | Meaning |
| --- | --- | --- |
| `VITE_AGORA_RPC_URL` | `http://127.0.0.1:8545/rpc` | Node JSON-RPC |
| `VITE_AGORA_POLL_MS` | `2000` | Tip poll interval |

Shared client: `apps/shared/light-client`.

### Optional CPU mining (sidecar)

Tauri does not bundle the miner yet. Run the RandomX sidecar against the same RPC:

```bash
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-miner-sidecar
```
