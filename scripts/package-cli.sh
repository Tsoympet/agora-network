#!/usr/bin/env bash
# Build and package CLI binaries for the host OS/arch (local mirror of release.yml).
# Output: dist/agora-cli-<os>-<arch>.tar.gz (or .zip on Windows via Git Bash)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS" in
  linux) ;;
  darwin) OS="macos" ;;
  *) echo "unsupported OS: $OS" >&2; exit 1 ;;
esac
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
esac

ASSET="agora-cli-${OS}-${ARCH}"
mkdir -p "dist/pkg"

echo "Building release binaries…"
if [[ "$(uname -s)" == "Linux" ]]; then
  export LIBCLANG_PATH="${LIBCLANG_PATH:-$(dirname "$(find /usr/lib -name 'libclang.so*' 2>/dev/null | head -1)")}"
fi

cargo build --release \
  -p agora-node \
  -p agora-layers \
  -p agora-miner-sidecar

for bin in agora-node agora-layers agora-miner; do
  cp "target/release/${bin}" "dist/pkg/"
done

tar -C dist/pkg -czf "dist/${ASSET}.tar.gz" agora-node agora-layers agora-miner
echo "Wrote dist/${ASSET}.tar.gz"
ls -lah "dist/${ASSET}.tar.gz"
