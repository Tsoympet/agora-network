#!/usr/bin/env bash
# Local Experimental Trident mesh — generated keys, not ceremony-final.
#
# Uses docs/genesis/trident.experimental.public-testnet.json.
# Does not touch frozen v2 datadirs or agora-trident-testnet-1 ceremony keys.
#
# Usage:
#   ./scripts/experimental_trident_mesh.sh wipe
#   ./scripts/experimental_trident_mesh.sh seeder
#   ./scripts/experimental_trident_mesh.sh node-a
#   ./scripts/experimental_trident_mesh.sh node-b
#   ./scripts/experimental_trident_mesh.sh wait-peers
#   ./scripts/experimental_trident_mesh.sh smoke-tx
#   ./scripts/experimental_trident_mesh.sh smoke-ibd
#   ./scripts/experimental_trident_mesh.sh smoke-finality
#
# Build first (this host may lack libstdc++ / rocksdb):
#   cargo build -p agora-node --no-default-features -p agora-dns-seeder
#   cargo build -p agora-miner-sidecar --no-default-features
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ARTIFACT="${AGORA_TRIDENT_GENESIS_FILE:-$ROOT/docs/genesis/trident.experimental.public-testnet.json}"
DATA_A="${AGORA_DATA_A:-$ROOT/data/experimental-trident-a}"
DATA_B="${AGORA_DATA_B:-$ROOT/data/experimental-trident-b}"
RPC_A="${AGORA_RPC_A:-http://127.0.0.1:8555/rpc}"
RPC_B="${AGORA_RPC_B:-http://127.0.0.1:8556/rpc}"
SEEDER_URL="${AGORA_DNS_SEEDER:-http://127.0.0.1:18082}"
SEEDER_BIND="${AGORA_SEEDER_BIND:-127.0.0.1:18082}"
LISTEN_A="${AGORA_LISTEN_A:-/ip4/127.0.0.1/tcp/16121}"
LISTEN_B="${AGORA_LISTEN_B:-/ip4/127.0.0.1/tcp/16122}"
SMOKE_TIMEOUT_SECS="${AGORA_SMOKE_TIMEOUT_SECS:-180}"
SMOKE_IBD_BLOCKS="${AGORA_SMOKE_IBD_BLOCKS:-1}"
# Experimental generated TLT allocation (secret 0xa3 repeating).
TLT_MINER="agoratest1e2eqe8pw7w094srjmulracnh7qn8xz9k4z8234"

run_bin() {
  local pkg="$1"
  local bin="$ROOT/target/debug/$pkg"
  if [[ -x "$bin" ]]; then
    echo "exec $bin"
    exec "$bin"
  fi
  echo "error: missing $bin — cargo build -p ${pkg/agora-miner/agora-miner-sidecar} --no-default-features" >&2
  exit 1
}

run_bin_fg() {
  local pkg="$1"
  local bin="$ROOT/target/debug/$pkg"
  if [[ -x "$bin" ]]; then
    echo "run $bin"
    "$bin"
    return $?
  fi
  echo "error: missing $bin" >&2
  return 1
}

export_experimental() {
  unset AGORA_GENESIS_FILE || true
  export AGORA_GENESIS_FILE=""
  export AGORA_TRIDENT_GENESIS_FILE="$ARTIFACT"
  export AGORA_NETWORK=testnet
  export AGORA_MINER_ADDRESS="${AGORA_MINER_ADDRESS:-$TLT_MINER}"
  export AGORA_MIN_RELAY_FEE="${AGORA_MIN_RELAY_FEE:-1}"
}

rpc_call() {
  local url="$1"
  local method="$2"
  curl -sS --connect-timeout 2 "$url" \
    -H 'content-type: application/json' \
    -d "{\"id\":1,\"method\":\"${method}\",\"params\":[]}" 2>/dev/null
}

tips_fingerprint() {
  local body
  body="$(rpc_call "$1" "agora_getDagTips")" || return 1
  python3 - "$body" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
tips = data.get("result") or []
print(",".join(sorted(tips)))
PY
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

wait_tips_converge() {
  local label="$1"
  local deadline=$((SECONDS + SMOKE_TIMEOUT_SECS))
  local after_a after_b
  after_a="$(tips_fingerprint "$RPC_A" || true)"
  if [[ -z "$after_a" ]]; then
    echo "error: could not read node-a tips during $label" >&2
    return 1
  fi
  echo "A tips ($label): $after_a"
  after_b=""
  while (( SECONDS < deadline )); do
    after_a="$(tips_fingerprint "$RPC_A" || true)"
    after_b="$(tips_fingerprint "$RPC_B" || true)"
    echo "  B tips: ${after_b:-?}  |  A tips: ${after_a:-?}"
    if [[ -n "$after_b" && -n "$after_a" && "$after_b" == "$after_a" ]]; then
      echo "$label OK — A and B tip sets match"
      return 0
    fi
    sleep 2
  done
  echo "error: timed out waiting for tip convergence ($label)" >&2
  echo "A=$after_a" >&2
  echo "B=${after_b:-}" >&2
  return 1
}

print_env() {
  cat <<EOF
Experimental Trident local mesh
───────────────────────────────
  artifact   = $ARTIFACT
  chain_id   = agora-trident-experimental-testnet-1
  node-a     DATA=$DATA_A  LISTEN=$LISTEN_A  RPC=$RPC_A
  node-b     DATA=$DATA_B  LISTEN=$LISTEN_B  RPC=$RPC_B
  seeder     $SEEDER_BIND
  AGORA_GENESIS_FILE is unset (v2 loader off)

Honest limits:
  --no-default-features node uses in-memory state + SHA-256 RandomX fallback
  unless agora-node was built with rocksdb+randomx (needs libstdc++).
  Not ceremony-final. Not Public testnet. Not mainnet.
EOF
}

cmd="${1:-help}"
case "$cmd" in
  help|-h|--help|"")
    print_env
    ;;
  wipe)
    echo "Removing $DATA_A and $DATA_B"
    rm -rf "$DATA_A" "$DATA_B"
    ;;
  seeder)
    export AGORA_SEEDER_BIND="$SEEDER_BIND"
    echo "DNS seeder on $SEEDER_BIND"
    run_bin agora-dns-seeder
    ;;
  node-a|a)
    export_experimental
    export AGORA_DATA="$DATA_A"
    export AGORA_LISTEN="$LISTEN_A"
    export AGORA_RPC_BIND="127.0.0.1:8555"
    export AGORA_DNS_SEEDER="$SEEDER_URL"
    export AGORA_SEEDER_REFRESH_SECS="${AGORA_SEEDER_REFRESH_SECS:-5}"
    export AGORA_RPC_ALLOW_FUND=0
    export AGORA_ARCHIVAL=1
    # Keep an explicit bootstrap if the caller set one (avoids first-boot
    # simultaneous-dial Noise failures against a just-started peer).
    if [[ -z "${AGORA_BOOTSTRAP:-}" ]]; then
      unset AGORA_BOOTSTRAP || true
    fi
    echo "experimental node-a  data=$AGORA_DATA  listen=$AGORA_LISTEN"
    mkdir -p "$DATA_A"
    run_bin agora-node
    ;;
  node-b|b)
    export_experimental
    export AGORA_DATA="$DATA_B"
    export AGORA_LISTEN="$LISTEN_B"
    export AGORA_RPC_BIND="127.0.0.1:8556"
    export AGORA_DNS_SEEDER="$SEEDER_URL"
    export AGORA_SEEDER_REFRESH_SECS="${AGORA_SEEDER_REFRESH_SECS:-5}"
    export AGORA_RPC_ALLOW_FUND=0
    if [[ -z "${AGORA_BOOTSTRAP:-}" ]]; then
      unset AGORA_BOOTSTRAP || true
    fi
    echo "experimental node-b  data=$AGORA_DATA  listen=$AGORA_LISTEN bootstrap=${AGORA_BOOTSTRAP:-seeder}"
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
    echo "=== seeder ($SEEDER_URL/peers) ==="
    curl -sS --connect-timeout 2 "$SEEDER_URL/peers" 2>/dev/null || echo "(unreachable)"
    echo
    ;;
  wait-peers)
    echo "Waiting for experimental A+B healthy and seeder ≥2 peers (timeout ${SMOKE_TIMEOUT_SECS}s)"
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
      (cd "$ROOT/apps/shared" && npm install --silent)
    fi
    export AGORA_RPC_A="$RPC_A"
    export AGORA_RPC_B="$RPC_B"
    export AGORA_SMOKE_TIMEOUT_SECS="$SMOKE_TIMEOUT_SECS"
    node --experimental-strip-types "$ROOT/scripts/experimental_trident_smoke.mjs" tx
    ;;
  smoke-ibd)
    require_health "$RPC_A" "node-a"
    require_health "$RPC_B" "node-b"
    before_a="$(tips_fingerprint "$RPC_A")"
    echo "before A tips: $before_a"
    echo "Mining $SMOKE_IBD_BLOCKS Experimental block(s) on node-a…"
    export AGORA_RPC_URL="$RPC_A"
    export AGORA_MINE_MAX_BLOCKS="$SMOKE_IBD_BLOCKS"
    export AGORA_MINE_POLL_MS="${AGORA_MINE_POLL_MS:-200}"
    run_bin_fg agora-miner
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
    echo "after A tips: $after_a"
    wait_tips_converge "Experimental IBD" || exit 1
    ;;
  smoke-finality)
    require_health "$RPC_A" "node-a"
    require_health "$RPC_B" "node-b"
    if [[ ! -d "$ROOT/apps/shared/node_modules/@noble/secp256k1" ]]; then
      (cd "$ROOT/apps/shared" && npm install --silent)
    fi
    export AGORA_RPC_A="$RPC_A"
    export AGORA_RPC_B="$RPC_B"
    node --experimental-strip-types "$ROOT/scripts/experimental_trident_smoke.mjs" finality
    ;;
  *)
    echo "Unknown command: $cmd" >&2
    print_env
    exit 1
    ;;
esac
