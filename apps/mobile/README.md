# Agora Mobile

Expo light-client wallet with HTTP JSON-RPC tip sync. Metro watches
`apps/shared` so light-client edits hot-reload without restarting the bundler.

## Features

- Clear **Devnet / Testnet / Mainnet** badge (from `agora_getNodeInfo`)
- Bech32m receive with network HRP (`agoradev` / `agoratest` / `agora`) + clipboard copy
- BIP-39 generate / derive (`m/44'/8888'/0'/0/0`) and signed send
- Password vault (AES-256-GCM) persisted via Expo SecureStore — Unlock / Save / Lock
- Compact node strip via `agora_getNodeInfo`
- Post-send pending → confirmed + confirmation depth + fee

```bash
# Terminal A
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# Terminal B
cd apps/mobile
npm install
npm start
npm run web               # requires react-dom / react-native-web / @expo/metro-runtime
# device builds: npm run android | npm run ios
# store/EAS: npm run build:android | npm run build:ios
```

iOS + Android packaging: [`docs/apps/PLATFORMS.md`](../../docs/apps/PLATFORMS.md).

| Env | Default | Meaning |
| --- | --- | --- |
| `EXPO_PUBLIC_AGORA_RPC_URL` | `http://127.0.0.1:8545/rpc` | Node JSON-RPC (**use LAN IP on a physical device**) |
| `EXPO_PUBLIC_AGORA_POLL_MS` | `2000` | Tip poll interval |

Shared client: `apps/shared/light-client`.
