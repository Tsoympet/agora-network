#!/usr/bin/env bash
# Install Agora Network CLI binaries from GitHub Releases.
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Tsoympet/agora-network/main/scripts/install.sh | bash
#   ./scripts/install.sh                 # latest release
#   ./scripts/install.sh v0.1.0          # specific tag
#   AGORA_INSTALL_DIR=~/bin ./scripts/install.sh
set -euo pipefail

REPO="${AGORA_REPO:-Tsoympet/agora-network}"
PREFIX="${AGORA_INSTALL_DIR:-${HOME}/.local/bin}"
VERSION="${1:-latest}"
BINS=(agora-node agora-layers agora-miner)

die() { echo "error: $*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

need curl
need tar
need uname

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS" in
  linux) ;;
  darwin) OS="macos" ;;
  mingw*|msys*|cygwin*) die "use the Windows .zip from GitHub Releases, or install via WSL" ;;
  *) die "unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) die "unsupported architecture: $ARCH" ;;
esac

ASSET="agora-cli-${OS}-${ARCH}.tar.gz"

if [[ "$VERSION" == "latest" ]]; then
  API="https://api.github.com/repos/${REPO}/releases/latest"
  TAG="$(curl -fsSL "$API" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
  [[ -n "$TAG" ]] || die "could not resolve latest release tag for ${REPO}"
else
  TAG="$VERSION"
fi

BASE="https://github.com/${REPO}/releases/download/${TAG}"
URL="${BASE}/${ASSET}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Installing Agora CLI ${TAG} (${OS}/${ARCH}) → ${PREFIX}"
echo "Downloading ${URL}"
HTTP_CODE="$(curl -fsSL -w '%{http_code}' -o "${TMP}/${ASSET}" "$URL" || true)"
if [[ "$HTTP_CODE" != "200" ]]; then
  die "download failed (HTTP ${HTTP_CODE}). Is ${TAG} published with ${ASSET}?"
fi

mkdir -p "$PREFIX"
tar -xzf "${TMP}/${ASSET}" -C "$TMP"
for bin in "${BINS[@]}"; do
  [[ -f "${TMP}/${bin}" ]] || die "archive missing ${bin}"
  install -m 0755 "${TMP}/${bin}" "${PREFIX}/${bin}"
  echo "  + ${PREFIX}/${bin}"
done

case ":$PATH:" in
  *":${PREFIX}:"*) ;;
  *)
    echo ""
    echo "Note: ${PREFIX} is not on your PATH. Add:"
    echo "  export PATH=\"${PREFIX}:\$PATH\""
    ;;
esac

echo ""
echo "Done. Verify with:"
echo "  agora-node --help"
echo "  agora-layers --help"
echo "  agora-miner --help"
