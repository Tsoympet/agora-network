# Agora Desktop

Tauri 2 + Vite wallet shell with HTTP JSON-RPC tip sync.
Native crate: `apps/desktop/src-tauri` (`agora-desktop`).

## Features

- Clear **Devnet / Testnet / Mainnet** badge (from `agora_getNodeInfo`)
- Bech32m receive with network HRP (`agoradev` / `agoratest` / `agora`) + hex secondary
- BIP-39 generate / derive (`m/44'/8888'/0'/0/0`) and signed send
- Password vault (AES-256-GCM) persisted in `localStorage` — Unlock / Save / Lock
- Compact node strip via `agora_getNodeInfo` (mempool, PoW, peers, archival)
- Post-send pending → confirmed + confirmation depth
- Whole-network light sync: selected-parent headers, TLT Merkle checks, DRC object query, OVL/DRC/TLT balances
- Device-to-device pairing: watch-only QR (public addresses, OVL/DRC account ids, account xpub) and a reveal-once mnemonic restore. Saved RPC URL overrides the env default.

```bash
# Terminal A
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# Terminal B
cd apps/desktop
npm install
npm run tauri:dev      # Vite + native shell
# or browser-only: npm run dev
npm run tauri:build    # host-OS installers (.deb / AppImage / NSIS / .dmg)
```

Prebuilt installers ship on GitHub Releases (`v*` tags). See
[`docs/apps/PLATFORMS.md`](../../docs/apps/PLATFORMS.md).


| Env | Default | Meaning |
| --- | --- | --- |
| `VITE_AGORA_RPC_URL` | `http://127.0.0.1:8545/rpc` | Node JSON-RPC until a saved endpoint replaces it |
| `VITE_AGORA_POLL_MS` | `2000` | Tip poll interval |

Shared client: `apps/shared/light-client`. Threat model: [`docs/core/agora-light-client.md`](../../docs/core/agora-light-client.md).

### Optional CPU mining (sidecar)

Tauri does not bundle the miner yet. Run the RandomX sidecar against the same RPC:

```bash
AGORA_RPC_URL=http://127.0.0.1:8545/rpc cargo run -p agora-miner-sidecar
```
