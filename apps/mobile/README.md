# Agora Mobile

Expo light-client shell with HTTP JSON-RPC tip sync.

```bash
# Terminal A
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# Terminal B
cd apps/mobile
npm install
npm start
```

| Env | Default | Meaning |
| --- | --- | --- |
| `EXPO_PUBLIC_AGORA_RPC_URL` | `http://127.0.0.1:8545/rpc` | Node JSON-RPC (use LAN IP on device) |
| `EXPO_PUBLIC_AGORA_POLL_MS` | `2000` | Tip poll interval |

Shared client: `apps/shared/light-client`.
