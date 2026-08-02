#!/bin/bash
set -euo pipefail

# Agora Network Monorepo Initialization Script

echo "Initializing Agora Network monorepo structure..."

mkdir -p apps/desktop apps/mobile apps/explorer
mkdir -p core/crates/{types,consensus,state-machine,p2p,rpc,crypto,miner-sidecar}
mkdir -p core/node-bin
mkdir -p infrastructure/{dns-seeder,stratum-pool,testnet-faucet}
mkdir -p docs/core scripts

echo "Directory structure ready."
echo "Next: cargo check --workspace"
echo "Docs: PROJECT_STRUCTURE.md, AGORA_MASTER_EXECUTION_ROADMAP.md, docs/core/"
