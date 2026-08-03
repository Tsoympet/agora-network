# Agora Mobile

Expo light-client wallet with HTTP JSON-RPC tip sync. Metro watches
`apps/shared` so light-client edits hot-reload without restarting the bundler.

## Features

- Bech32m receive (`agora1…`) with hex secondary + clipboard copy
- BIP-39 generate / derive (`m/44'/8888'/0'/0/0`) and signed send
- Compact node strip via `agora_getNodeInfo`
- Post-send pending → confirmed + confirmation depth + fee

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
| `EXPO_PUBLIC_AGORA_RPC_URL` | `http://127.0.0.1:8545/rpc` | Node JSON-RPC (**use LAN IP on a physical device**) |
| `EXPO_PUBLIC_AGORA_POLL_MS` | `2000` | Tip poll interval |

Shared client: `apps/shared/light-client`.
