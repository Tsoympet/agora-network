# Agora apps — platforms & installers

Wallets and the explorer must always show which chain the connected node is on:
**Devnet**, **Testnet**, or **Mainnet** (from `agora_getNodeInfo.network`).
Receive/send Bech32 HRPs follow that network (`agoradev` / `agoratest` / `agora`).

## CLI binaries (node / layers / miner)

GitHub Releases (tag `v*`) publish OS/arch archives built by
[`.github/workflows/release.yml`](../../.github/workflows/release.yml):

| Asset | Contents |
| --- | --- |
| `agora-cli-linux-x86_64.tar.gz` | `agora-node`, `agora-layers`, `agora-miner` |
| `agora-cli-linux-aarch64.tar.gz` | same |
| `agora-cli-macos-x86_64.tar.gz` | same |
| `agora-cli-macos-aarch64.tar.gz` | same |
| `agora-cli-windows-x86_64.zip` | `.exe` variants |

### One-line install (Linux / macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/Tsoympet/agora-network/main/scripts/install.sh | bash
# pin a tag:
# curl -fsSL …/scripts/install.sh | bash -s -- v0.1.0
```

Installs into `~/.local/bin` (override with `AGORA_INSTALL_DIR`).

Local packaging (current host only):

```bash
./scripts/package-cli.sh   # → dist/agora-cli-<os>-<arch>.tar.gz
```

## Desktop (all PC systems)

Stack: **Tauri 2** + React/Vite (`apps/desktop`). Full Rust shell under `apps/desktop/src-tauri/`.

| OS | Installer targets |
| --- | --- |
| Linux | `.deb`, AppImage (also `.rpm` via local `build:linux`) |
| Windows | NSIS (`.exe`); MSI via local `build:windows` |
| macOS | `.dmg` (also `.app` via local `build:macos`) |

Release workflow uploads unsigned installers alongside CLI archives.
**Code signing** (Apple Developer, Windows Authenticode) is operator-owned — Gatekeeper / SmartScreen warnings are expected until secrets are wired.

```bash
cd apps/desktop
npm install
npm run tauri:dev      # Vite + native shell
npm run tauri:build    # host-OS installers
# or:
npm run build:linux    # deb + appimage + rpm
npm run build:windows  # nsis + msi
npm run build:macos    # app + dmg
```

Linux system deps (Debian/Ubuntu):

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential
```

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
npm run web               # Expo web
npm run android           # device / emulator
npm run ios               # Simulator / device (macOS)
npm run build:android     # EAS (requires `eas login` + project link)
npm run build:ios
npm run build:all
```

`eas.json` profiles: `development`, `preview`, `production`.

## Explorer

Web SPA (`apps/explorer`) — any modern browser on PC or phone. Header badge shows live network.

## Publishing a release

```bash
git tag v0.1.0
git push origin v0.1.0
# or: Actions → Release → Run workflow
```

## Readiness

| Layer | Status |
| --- | --- |
| In-repo UI + HRP wiring | yes |
| Tauri native shell + bundle config | yes |
| GitHub Release CLI + desktop artifacts | yes (on `v*` tags) |
| `scripts/install.sh` | yes |
| Public store listing + signing | ops / developer accounts (not in-repo) |
| Homebrew / winget / apt repo | not yet |
| Mainnet value | blocked until genesis freeze + review |
