#!/usr/bin/env bash
# Prepare / sanity-check mainnet genesis freeze inputs.
# Does NOT publish a frozen hash — edit ChainParams::mainnet() first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DRAFT="docs/genesis/mainnet.genesis.draft.json"
OUT="docs/genesis/mainnet.genesis.json"

echo "== Agora mainnet genesis prep =="
echo

if [[ ! -f "$DRAFT" ]]; then
  echo "missing $DRAFT — run from repo with Phase 32 genesis docs"
  exit 1
fi

echo "Checklist:"
echo "  [ ] SLIP-0044 registered or provisional risk accepted (docs/governance/SLIP0044.md)"
echo "  [ ] Premine address + timestamp + bits set in ChainParams::mainnet()"
echo "  [ ] for_network(Mainnet) allows boot with expected_genesis"
echo "  [ ] Token caps unchanged (TLT 100M / DRC 6B / OVL 21B)"
echo

if ! grep -q '"status": "DRAFT' "$DRAFT" 2>/dev/null; then
  echo "note: draft status marker not found in $DRAFT"
fi

echo "Draft token registry:"
python3 - <<'PY'
import json
from pathlib import Path
p = Path("docs/genesis/mainnet.genesis.draft.json")
data = json.loads(p.read_text())
for t in data.get("tokens", []):
    whole = t["max_supply"] / (10 ** t.get("decimals", 8))
    print(f"  {t['ticker']}: {whole:,.0f} max ({t['layer']}) — {t['role']}")
wallet = data.get("wallet") or {}
print(f"  HRP: {wallet.get('address_hrp')}  coin_type: {wallet.get('coin_type')} ({wallet.get('coin_type_status')})")
PY

echo
echo "When ChainParams::mainnet() is ready:"
echo "  cargo run -p agora-node -- genesis dump --network mainnet --out $OUT"
echo "  cargo run -p agora-node -- genesis verify --network mainnet --file $OUT"
echo
echo "See docs/governance/MAINNET_GENESIS_FREEZE.md"
