#!/usr/bin/env bash
# Local Agora testnet runbook — boots a funded node plus companion services.
#
# Usage:
#   ./scripts/local_testnet.sh              # print env + start commands
#   ./scripts/local_testnet.sh up           # start node (foreground) with testnet defaults
#   ./scripts/local_testnet.sh faucet       # start faucet (needs node with ALLOW_FUND)
#   ./scripts/local_testnet.sh stratum      # start stratum (needs kheavyhash node)
#   ./scripts/local_testnet.sh miner        # start RandomX miner sidecar
#   ./scripts/local_testnet.sh wipe         # delete AGORA_DATA so premine can be re-ignited
#
# Derive a premine / miner address from the abandon mnemonic (external index 0):
#   ff9ec96f09eb154d038a552ecae59c50204ea9a9
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DATA_DIR="${AGORA_DATA:-$ROOT/data/agora-local-testnet}"
PREMINE="${AGORA_PREMINE_ADDRESS:-ff9ec96f09eb154d038a552ecae59c50204ea9a9}"
MINER="${AGORA_MINER_ADDRESS:-$PREMINE}"
RPC_BIND="${AGORA_RPC_BIND:-127.0.0.1:8545}"
RPC_URL="${AGORA_RPC_URL:-http://${RPC_BIND}/rpc}"
POW_ALGO="${AGORA_POW_ALGO:-randomx}"
TEMPLATE_BITS="${AGORA_TEMPLATE_BITS:-1}"
MIN_RELAY="${AGORA_MIN_RELAY_FEE:-1}"

export AGORA_DATA="$DATA_DIR"
export AGORA_PREMINE_ADDRESS="$PREMINE"
export AGORA_MINER_ADDRESS="$MINER"
export AGORA_RPC_BIND="$RPC_BIND"
export AGORA_RPC_URL="$RPC_URL"
export AGORA_POW_ALGO="$POW_ALGO"
export AGORA_TEMPLATE_BITS="$TEMPLATE_BITS"
export AGORA_MIN_RELAY_FEE="$MIN_RELAY"
export AGORA_RPC_ALLOW_FUND="${AGORA_RPC_ALLOW_FUND:-1}"

print_env() {
  cat <<EOF
Agora local testnet
───────────────────
AGORA_DATA            = $AGORA_DATA
AGORA_PREMINE_ADDRESS = $AGORA_PREMINE_ADDRESS
AGORA_MINER_ADDRESS   = $AGORA_MINER_ADDRESS
AGORA_RPC_BIND        = $AGORA_RPC_BIND
AGORA_RPC_URL         = $AGORA_RPC_URL
AGORA_POW_ALGO        = $AGORA_POW_ALGO
AGORA_TEMPLATE_BITS   = $AGORA_TEMPLATE_BITS
AGORA_MIN_RELAY_FEE   = $AGORA_MIN_RELAY_FEE
AGORA_RPC_ALLOW_FUND  = $AGORA_RPC_ALLOW_FUND

Suggested flow:
  1. ./scripts/local_testnet.sh wipe    # optional fresh genesis
  2. ./scripts/local_testnet.sh up      # node (RandomX) or POW_ALGO=kheavyhash
  3. ./scripts/local_testnet.sh faucet  # other terminal
  4. ./scripts/local_testnet.sh miner   # RandomX sidecar
     # or: POW_ALGO=kheavyhash ./scripts/local_testnet.sh up
     #     ./scripts/local_testnet.sh stratum
  5. Clients:
       cd apps/explorer && npm run dev
       cd apps/desktop  && npm run dev
       # VITE_AGORA_RPC_URL=$RPC_URL

Premine mnemonic (abandon…about) external(0) → $PREMINE
Send UI: paste mnemonic, derive, fund via faucet if needed, sign & send (fee ≥ $MIN_RELAY).
EOF
}

cmd="${1:-help}"
case "$cmd" in
  help|-h|--help|"")
    print_env
    ;;
  wipe)
    echo "Removing $DATA_DIR"
    rm -rf "$DATA_DIR"
    ;;
  up|node)
    print_env
    mkdir -p "$DATA_DIR"
    exec cargo run -p agora-node
    ;;
  faucet)
    exec cargo run -p agora-testnet-faucet
    ;;
  stratum)
    echo "Node should run with AGORA_POW_ALGO=kheavyhash"
    exec cargo run -p agora-stratum-pool
    ;;
  miner)
    exec cargo run -p agora-miner-sidecar
    ;;
  *)
    echo "Unknown command: $cmd" >&2
    print_env
    exit 1
    ;;
esac
