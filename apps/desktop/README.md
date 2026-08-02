# Agora Desktop

Tauri + Vite wallet shell (Obsidian & Gold) with HTTP JSON-RPC tip sync.

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
