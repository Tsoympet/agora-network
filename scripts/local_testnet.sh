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
#   ./scripts/local_testnet.sh tips
#   ./scripts/local_testnet.sh smoke-ibd    # mine 1 block on A, wait for B tip converge
#   ./scripts/local_testnet.sh smoke-tx     # signed send on A → pending on B (tx gossip)
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
SMOKE_TIMEOUT_SECS="${AGORA_SMOKE_TIMEOUT_SECS:-180}"

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

# Run a binary in the foreground without replacing this shell (for smoke helpers).
run_bin_fg() {
  local pkg="$1"
  local bin="$ROOT/target/debug/$pkg"
  if [[ -x "$bin" ]]; then
    echo "run $bin"
    "$bin"
    return $?
  fi
  echo "run cargo run -p $pkg"
  cargo run -q -p "$pkg"
}

rpc_call() {
  local url="$1"
  local method="$2"
  curl -sS --connect-timeout 2 "$url" \
    -H 'content-type: application/json' \
    -d "{\"id\":1,\"method\":\"${method}\",\"params\":[]}" 2>/dev/null
}

# Print sorted tip hashes (one per line) from an agora_getDagTips JSON response.
tips_list() {
  local url="$1"
  local body
  body="$(rpc_call "$url" "agora_getDagTips")" || return 1
  python3 - "$body" <<'PY'
import json, sys
raw = sys.argv[1]
data = json.loads(raw)
tips = data.get("result") or []
for t in sorted(tips):
    print(t)
PY
}

tips_fingerprint() {
  tips_list "$1" | paste -sd, -
}

require_health() {
  local url="$1"
  local name="$2"
  local health
  health="$(curl -sS --connect-timeout 2 "${url%/rpc}/health" 2>/dev/null || true)"
  if [[ "$health" != *ok* ]]; then
    echo "error: $name not healthy at ${url%/rpc}/health" >&2
    return 1
  fi
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

Suggested two-node IBD + tx gossip smoke:
  0. cargo build -p agora-dns-seeder -p agora-node -p agora-miner-sidecar
     (cd apps/shared && npm install)   # once, for smoke-tx light-client
  1. ./scripts/local_testnet.sh wipe-two
  2. ./scripts/local_testnet.sh seeder
  3. ./scripts/local_testnet.sh node-a
  4. ./scripts/local_testnet.sh node-b
  5. ./scripts/local_testnet.sh wait-peers
  6. ./scripts/local_testnet.sh smoke-tx    # signed premine spend on A → pending on B
  7. ./scripts/local_testnet.sh smoke-ibd   # mines 1 block on A, waits for B

Premine mnemonic (abandon…about) external(0) → $PREMINE
Note: agora_fundAddress is local mint only — use smoke-tx / mined blocks to prove gossip/IBD.
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
  wait-peers)
    echo "Waiting for node-a + node-b healthy and seeder listing ≥2 peers (timeout ${SMOKE_TIMEOUT_SECS}s)"
    deadline=$((SECONDS + SMOKE_TIMEOUT_SECS))
    while (( SECONDS < deadline )); do
      ha=$(curl -sS --connect-timeout 1 "${RPC_A%/rpc}/health" 2>/dev/null || true)
      hb=$(curl -sS --connect-timeout 1 "${RPC_B%/rpc}/health" 2>/dev/null || true)
      peers=$(curl -sS --connect-timeout 1 "$SEEDER_URL/peers" 2>/dev/null || true)
      count=$(python3 -c 'import json,sys; print(len(json.loads(sys.argv[1])))' "$peers" 2>/dev/null || echo 0)
      echo "health_a=${ha:-?} health_b=${hb:-?} peer_count=$count"
      if [[ "$ha" == *ok* && "$hb" == *ok* && "$count" -ge 2 ]]; then
        echo "peers ready"
        exit 0
      fi
      sleep 2
    done
    echo "error: timed out waiting for peers" >&2
    exit 1
    ;;
  smoke-tx)
    require_health "$RPC_A" "node-a"
    require_health "$RPC_B" "node-b"
    if [[ ! -d "$ROOT/apps/shared/node_modules/@noble/secp256k1" ]]; then
      echo "Installing light-client deps in apps/shared…"
      (cd "$ROOT/apps/shared" && npm install --silent)
    fi
    export AGORA_RPC_A="$RPC_A"
    export AGORA_RPC_B="$RPC_B"
    export AGORA_SMOKE_TIMEOUT_SECS="$SMOKE_TIMEOUT_SECS"
    echo "Submitting premine transfer on A and waiting for B mempool…"
    node --experimental-strip-types "$ROOT/scripts/smoke_tx.mjs"
    ;;
  smoke-ibd)
    require_health "$RPC_A" "node-a"
    require_health "$RPC_B" "node-b"
    before_a="$(tips_fingerprint "$RPC_A")"
    before_b="$(tips_fingerprint "$RPC_B")"
    echo "before A tips: $before_a"
    echo "before B tips: $before_b"
    if [[ -z "$before_a" || -z "$before_b" ]]; then
      echo "error: could not read tips from both nodes" >&2
      exit 1
    fi
    if [[ "$before_a" != "$before_b" ]]; then
      echo "warn: A/B tips already diverge; continuing IBD smoke anyway" >&2
    fi

    echo "Mining 1 block on node-a (AGORA_MINE_MAX_BLOCKS=1, bits≈$TEMPLATE_BITS)…"
    export AGORA_RPC_URL="$RPC_A"
    export AGORA_MINE_MAX_BLOCKS=1
    export AGORA_MINE_POLL_MS="${AGORA_MINE_POLL_MS:-500}"
    run_bin_fg agora-miner-sidecar

    echo "Waiting for node-a tip to advance…"
    deadline=$((SECONDS + SMOKE_TIMEOUT_SECS))
    after_a=""
    while (( SECONDS < deadline )); do
      after_a="$(tips_fingerprint "$RPC_A" || true)"
      if [[ -n "$after_a" && "$after_a" != "$before_a" ]]; then
        break
      fi
      sleep 1
    done
    if [[ -z "$after_a" || "$after_a" == "$before_a" ]]; then
      echo "error: node-a tips did not advance after mine" >&2
      exit 1
    fi
    echo "after  A tips: $after_a"

    echo "Waiting for node-b tips to converge with node-a…"
    after_b=""
    while (( SECONDS < deadline )); do
      after_b="$(tips_fingerprint "$RPC_B" || true)"
      echo "  B tips: ${after_b:-?}"
      if [[ -n "$after_b" && "$after_b" == "$after_a" ]]; then
        echo "IBD smoke OK — A and B tip sets match after mined block"
        exit 0
      fi
      # Also accept B containing every tip from A (subset equality for multi-tip cases).
      if [[ -n "$after_b" ]] && python3 - "$after_a" "$after_b" <<'PY'
import sys
a=set(sys.argv[1].split(',')) if sys.argv[1] else set()
b=set(sys.argv[2].split(',')) if sys.argv[2] else set()
sys.exit(0 if a and a <= b else 1)
PY
      then
        echo "IBD smoke OK — B contains all of A's tips"
        exit 0
      fi
      sleep 2
    done
    echo "error: timed out waiting for tip convergence" >&2
    echo "A=$after_a" >&2
    echo "B=${after_b:-}" >&2
    exit 1
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
    echo "miner → $AGORA_RPC_URL (AGORA_MINE_MAX_BLOCKS=${AGORA_MINE_MAX_BLOCKS:-0})"
    run_bin agora-miner-sidecar
    ;;
  *)
    echo "Unknown command: $cmd" >&2
    print_env
    exit 1
    ;;
esac
