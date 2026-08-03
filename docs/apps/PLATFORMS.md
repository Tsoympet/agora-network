# Agora apps — platforms & network indicator

Wallets and the explorer must always show which chain the connected node is on:
**Devnet**, **Testnet**, or **Mainnet** (from `agora_getNodeInfo.network`).
Receive/send Bech32 HRPs follow that network (`agoradev` / `agoratest` / `agora`).

## Desktop (all PC systems)

Stack: **Tauri 2** + React/Vite (`apps/desktop`).

| OS | Installer targets |
| --- | --- |
| Linux | `.deb`, `.rpm`, AppImage |
| Windows | NSIS (`.exe`), MSI |
| macOS | `.app`, `.dmg` |

```bash
cd apps/desktop
npm install
npm run build          # web assets
npm run tauri:dev      # native shell (dev)
npm run tauri:build    # native installers for the host OS
```

Cross-OS note: Tauri produces installers for the **machine you build on**
(or a CI matrix: `ubuntu-latest` / `windows-latest` / `macos-latest`).
Code signing (Apple Developer, Windows Authenticode) is required for store/gatekeeper trust — operator-owned.

## Mobile (phone systems)

Stack: **Expo** + React Native (`apps/mobile`).

| OS | Path |
| --- | --- |
| Android | `expo run:android` / EAS → APK or Play AAB |
| iOS | `expo run:ios` (macOS + Xcode) / EAS → IPA; App Store needs Apple Developer |

```bash
cd apps/mobile
npm install
npm start                 # Expo Metro
npm run android           # device / emulator
npm run ios               # Simulator / device (macOS)
npm run build:android     # EAS (requires `eas login` + project link)
npm run build:ios
npm run build:all
```

`eas.json` profiles: `development`, `preview`, `production`.

## Explorer

Web SPA (`apps/explorer`) — any modern browser on PC or phone. Header badge shows live network.

## Readiness

| Layer | Status |
| --- | --- |
| In-repo UI + HRP wiring | yes |
| Local/dev native builds | yes (Tauri / Expo on each host) |
| Public store listing + signing | ops / developer accounts (not in-repo) |
| Mainnet value | blocked until genesis freeze + review |
