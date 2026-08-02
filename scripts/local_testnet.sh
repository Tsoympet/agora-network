#!/usr/bin/env bash
# Local Agora testnet runbook — single-node helpers + two-node gossip/IBD smoke.
#
# Usage:
#   ./scripts/local_testnet.sh              # print env + start commands
#   ./scripts/local_testnet.sh up           # single node (foreground)
#   ./scripts/local_testnet.sh faucet       # faucet (needs ALLOW_FUND)
#   ./scripts/local_testnet.sh stratum      # stratum (needs kheavyhash node)
#   ./scripts/local_testnet.sh miner        # RandomX miner → AGORA_RPC_URL
#   ./scripts/local_testnet.sh wipe         # delete single-node AGORA_DATA
#
# Two-node gossip / IBD:
#   ./scripts/local_testnet.sh wipe-two
#   ./scripts/local_testnet.sh seeder       # terminal 0
#   ./scripts/local_testnet.sh node-a       # terminal 1 (RPC :8545, fund enabled)
#   ./scripts/local_testnet.sh node-b       # terminal 2 (RPC :8546)
#   AGORA_RPC_URL=http://127.0.0.1:8545/rpc ./scripts/local_testnet.sh miner
#   ./scripts/local_testnet.sh tips         # compare A vs B tips
#
# Premine mnemonic (abandon…about) external(0):
#   ff9ec96f09eb154d038a552ecae59c50204ea9a9
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DATA_DIR="${AGORA_DATA:-$ROOT/data/agora-local-testnet}"
DATA_A="${AGORA_DATA_A:-$ROOT/data/agora-node-a}"
DATA_B="${AGORA_DATA_B:-$ROOT/data/agora-node-b}"
PREMINE="${AGORA_PREMINE_ADDRESS:-ff9ec96f09eb154d038a552ecae59c50204ea9a9}"
MINER="${AGORA_MINER_ADDRESS:-$PREMINE}"
RPC_BIND="${AGORA_RPC_BIND:-127.0.0.1:8545}"
RPC_URL="${AGORA_RPC_URL:-http://${RPC_BIND}/rpc}"
RPC_A="${AGORA_RPC_A:-http://127.0.0.1:8545/rpc}"
RPC_B="${AGORA_RPC_B:-http://127.0.0.1:8546/rpc}"
SEEDER_URL="${AGORA_DNS_SEEDER:-http://127.0.0.1:18080}"
SEEDER_BIND="${AGORA_SEEDER_BIND:-127.0.0.1:18080}"
POW_ALGO="${AGORA_POW_ALGO:-randomx}"
TEMPLATE_BITS="${AGORA_TEMPLATE_BITS:-1}"
MIN_RELAY="${AGORA_MIN_RELAY_FEE:-1}"
LISTEN_A="${AGORA_LISTEN_A:-/ip4/127.0.0.1/tcp/16111}"
LISTEN_B="${AGORA_LISTEN_B:-/ip4/127.0.0.1/tcp/16112}"

export_common() {
  export AGORA_PREMINE_ADDRESS="$PREMINE"
  export AGORA_MINER_ADDRESS="$MINER"
  export AGORA_POW_ALGO="$POW_ALGO"
  export AGORA_TEMPLATE_BITS="$TEMPLATE_BITS"
  export AGORA_MIN_RELAY_FEE="$MIN_RELAY"
}

# Prefer prebuilt binaries (avoids parallel rocksdb rebuild races across terminals).
run_bin() {
  local pkg="$1"
  local bin="$ROOT/target/debug/$pkg"
  if [[ -x "$bin" ]]; then
    echo "exec $bin"
    exec "$bin"
  fi
  echo "exec cargo run -p $pkg (build if needed)"
  exec cargo run -p "$pkg"
}

rpc_call() {
  local url="$1"
  local method="$2"
  curl -sS --connect-timeout 2 "$url" \
    -H 'content-type: application/json' \
    -d "{\"id\":1,\"method\":\"${method}\",\"params\":[]}" 2>/dev/null
}

print_env() {
  cat <<EOF
Agora local testnet
───────────────────
Single node:
  AGORA_DATA            = $DATA_DIR
  AGORA_RPC_BIND        = $RPC_BIND
  AGORA_RPC_URL         = $RPC_URL
  AGORA_PREMINE_ADDRESS = $PREMINE
  AGORA_MINER_ADDRESS   = $MINER
  AGORA_POW_ALGO        = $POW_ALGO
  AGORA_TEMPLATE_BITS   = $TEMPLATE_BITS
  AGORA_MIN_RELAY_FEE   = $MIN_RELAY
  AGORA_RPC_ALLOW_FUND  = ${AGORA_RPC_ALLOW_FUND:-1}

Two-node gossip/IBD:
  seeder  AGORA_SEEDER_BIND     = $SEEDER_BIND
  node-a  DATA=$DATA_A  LISTEN=$LISTEN_A  RPC=127.0.0.1:8545  DNS_SEEDER=$SEEDER_URL
  node-b  DATA=$DATA_B  LISTEN=$LISTEN_B  RPC=127.0.0.1:8546  DNS_SEEDER=$SEEDER_URL

Suggested single-node flow:
  1. ./scripts/local_testnet.sh wipe
  2. ./scripts/local_testnet.sh up
  3. ./scripts/local_testnet.sh faucet    # other terminal
  4. ./scripts/local_testnet.sh miner
  5. Clients: VITE_AGORA_RPC_URL=$RPC_URL npm run dev (explorer/desktop)

Suggested two-node flow:
  0. ./scripts/local_testnet.sh wipe-two
  1. ./scripts/local_testnet.sh seeder
  2. ./scripts/local_testnet.sh node-a    # wait for "registered dialable addr"
  3. curl -s $SEEDER_URL/peers
  4. ./scripts/local_testnet.sh node-b    # expect "peer connected" on both
  5. AGORA_RPC_URL=$RPC_A ./scripts/local_testnet.sh miner
  6. ./scripts/local_testnet.sh tips      # A and B tip sets should match after gossip/IBD

Premine mnemonic (abandon…about) external(0) → $PREMINE
Note: agora_fundAddress is local mint only — use mined blocks to prove gossip.
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
  wipe-two)
    echo "Removing $DATA_A and $DATA_B"
    rm -rf "$DATA_A" "$DATA_B"
    ;;
  up|node)
    export_common
    export AGORA_DATA="$DATA_DIR"
    export AGORA_RPC_BIND="$RPC_BIND"
    export AGORA_RPC_URL="$RPC_URL"
    export AGORA_RPC_ALLOW_FUND="${AGORA_RPC_ALLOW_FUND:-1}"
    print_env
    mkdir -p "$DATA_DIR"
    run_bin agora-node
    ;;
  seeder)
    export AGORA_SEEDER_BIND="$SEEDER_BIND"
    echo "DNS seeder on $SEEDER_BIND"
    run_bin agora-dns-seeder
    ;;
  node-a|a)
    export_common
    export AGORA_DATA="$DATA_A"
    export AGORA_LISTEN="$LISTEN_A"
    export AGORA_RPC_BIND="127.0.0.1:8545"
    export AGORA_RPC_URL="$RPC_A"
    export AGORA_DNS_SEEDER="$SEEDER_URL"
    export AGORA_SEEDER_REFRESH_SECS="${AGORA_SEEDER_REFRESH_SECS:-5}"
    export AGORA_RPC_ALLOW_FUND="${AGORA_RPC_ALLOW_FUND:-1}"
    unset AGORA_BOOTSTRAP || true
    echo "node-a  data=$AGORA_DATA  listen=$AGORA_LISTEN  rpc=$AGORA_RPC_BIND  seeder=$AGORA_DNS_SEEDER"
    mkdir -p "$DATA_A"
    run_bin agora-node
    ;;
  node-b|b)
    export_common
    export AGORA_DATA="$DATA_B"
    export AGORA_LISTEN="$LISTEN_B"
    export AGORA_RPC_BIND="127.0.0.1:8546"
    export AGORA_RPC_URL="$RPC_B"
    export AGORA_DNS_SEEDER="$SEEDER_URL"
    export AGORA_SEEDER_REFRESH_SECS="${AGORA_SEEDER_REFRESH_SECS:-5}"
    export AGORA_RPC_ALLOW_FUND=0
    unset AGORA_BOOTSTRAP || true
    echo "node-b  data=$AGORA_DATA  listen=$AGORA_LISTEN  rpc=$AGORA_RPC_BIND  seeder=$AGORA_DNS_SEEDER"
    mkdir -p "$DATA_B"
    run_bin agora-node
    ;;
  tips)
    echo "=== node-a ($RPC_A) ==="
    rpc_call "$RPC_A" "agora_getDagTips" || echo "(unreachable)"
    echo
    echo "=== node-b ($RPC_B) ==="
    rpc_call "$RPC_B" "agora_getDagTips" || echo "(unreachable)"
    echo
    echo "=== seeder peers ($SEEDER_URL/peers) ==="
    curl -sS --connect-timeout 2 "$SEEDER_URL/peers" 2>/dev/null || echo "(unreachable)"
    echo
    ;;
  faucet)
    export AGORA_RPC_URL="${AGORA_RPC_URL:-$RPC_A}"
    run_bin agora-testnet-faucet
    ;;
  stratum)
    echo "Node should run with AGORA_POW_ALGO=kheavyhash"
    export AGORA_RPC_URL="${AGORA_RPC_URL:-$RPC_A}"
    run_bin agora-stratum-pool
    ;;
  miner)
    export AGORA_RPC_URL="${AGORA_RPC_URL:-$RPC_URL}"
    echo "miner → $AGORA_RPC_URL"
    run_bin agora-miner-sidecar
    ;;
  *)
    echo "Unknown command: $cmd" >&2
    print_env
    exit 1
    ;;
esac
