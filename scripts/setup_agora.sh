#!/usr/bin/env bash
# Legacy helper — the monorepo already contains the full tree.
# Prefer: cargo check --workspace  /  ./scripts/local_testnet.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "Agora Network workspace: $ROOT"
echo "Docs: README.md · PROJECT_STRUCTURE.md · docs/ops/PUBLIC_TESTNET.md"
echo ""
echo "Quick checks:"
echo "  cargo check --workspace"
echo "  cargo run -p agora-node -- genesis verify --network testnet"
echo "  ./scripts/local_testnet.sh"
